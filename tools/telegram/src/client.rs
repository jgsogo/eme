//! Shared Telegram gateway.
//!
//! Exactly one instance owns Telegram's update stream. Tools and inbound
//! adapters subscribe to its dispatcher instead of calling getUpdates
//! independently.

use std::{sync::Arc, time::Duration};

use frankenstein::{
    AsyncTelegramApi,
    client_reqwest::Bot,
    methods::{
        AnswerCallbackQueryParams, GetUpdatesParams, SendMessageParams,
    },
    types::{
        AllowedUpdate, InlineKeyboardButton, InlineKeyboardMarkup,
        MaybeInaccessibleMessage, ReplyMarkup,
    },
    updates::UpdateContent,
};
use thiserror::Error;
use tokio::sync::{Mutex, broadcast, oneshot};

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const POLL_TIMEOUT_SECS: u32 = 20;
const EVENT_CAPACITY: usize = 256;

#[derive(Error, Debug)]
pub enum TelegramError {
    #[error("TELEGRAM_BOT_TOKEN env var not set")]
    MissingToken,
    #[error("TELEGRAM_CHAT_ID env var not set or invalid")]
    MissingChatId,
    #[error("Telegram API error: {0}")]
    Api(String),
    #[error("Timed out waiting for user reply after {0:?}")]
    Timeout(Duration),
    #[error("Telegram update dispatcher is closed")]
    DispatcherClosed,
}

#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub chat_id: i64,
    pub user_id: Option<i64>,
    pub is_bot: bool,
    pub message_id: i32,
    pub text: Option<String>,
}

enum PendingReply {
    Text {
        chat_id: i64,
        sender: oneshot::Sender<String>,
    },
    YesNo {
        chat_id: i64,
        sender: oneshot::Sender<bool>,
    },
}

/// Owns the Bot API client and the sole getUpdates polling loop.
///
/// Create one gateway per bot token and share its Arc with tools and inbound
/// adapters. The gateway routes replies to an active interactive tool first;
/// all other messages are broadcast to normal message handlers.
pub struct TelegramGateway {
    bot: Bot,
    pending: Mutex<Option<PendingReply>>,
    interactive_lock: Mutex<()>,
    events: broadcast::Sender<IncomingMessage>,
}

impl TelegramGateway {
    pub fn new(token: &str) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Arc::new(Self {
            bot: Bot::new(token),
            pending: Mutex::new(None),
            interactive_lock: Mutex::new(()),
            events,
        })
    }

    pub fn from_env() -> Result<Arc<Self>, TelegramError> {
        let token = std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| TelegramError::MissingToken)?;
        Ok(Self::new(&token))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.events.subscribe()
    }

    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<(), TelegramError> {
        let params = SendMessageParams::builder()
            .chat_id(chat_id)
            .text(text)
            .build();
        self.bot.send_message(&params).await
            .map_err(|e| TelegramError::Api(e.to_string()))?;
        Ok(())
    }

    pub async fn notify_configured_chat(&self, text: &str) -> Result<(), TelegramError> {
        let chat_id = configured_chat_id()?;
        self.send_message(chat_id, text).await
    }

    pub async fn ask_text(
        &self,
        chat_id: i64,
        question: &str,
        timeout: Duration,
    ) -> Result<String, TelegramError> {
        let _guard = self.interactive_lock.lock().await;
        let (sender, receiver) = oneshot::channel();
        *self.pending.lock().await = Some(PendingReply::Text { chat_id, sender });

        if let Err(error) = self.send_message(chat_id, question).await {
            self.pending.lock().await.take();
            return Err(error);
        }

        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(_)) => Err(TelegramError::DispatcherClosed),
            Err(_) => {
                self.pending.lock().await.take();
                Err(TelegramError::Timeout(timeout))
            }
        }
    }

    pub async fn ask_yes_no(
        &self,
        chat_id: i64,
        question: &str,
        timeout: Duration,
    ) -> Result<bool, TelegramError> {
        let _guard = self.interactive_lock.lock().await;
        let (sender, receiver) = oneshot::channel();
        *self.pending.lock().await = Some(PendingReply::YesNo { chat_id, sender });

        let keyboard = InlineKeyboardMarkup {
            inline_keyboard: vec![vec![
                InlineKeyboardButton::builder()
                    .text("Yes")
                    .callback_data("yes")
                    .build(),
                InlineKeyboardButton::builder()
                    .text("No")
                    .callback_data("no")
                    .build(),
            ]],
            force_reply: None,
        };
        let params = SendMessageParams::builder()
            .chat_id(chat_id)
            .text(question)
            .reply_markup(ReplyMarkup::InlineKeyboardMarkup(keyboard))
            .build();

        if let Err(error) = self.bot.send_message(&params).await {
            self.pending.lock().await.take();
            return Err(TelegramError::Api(error.to_string()));
        }

        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(_)) => Err(TelegramError::DispatcherClosed),
            Err(_) => {
                self.pending.lock().await.take();
                Err(TelegramError::Timeout(timeout))
            }
        }
    }

    /// Start the only update poller for this bot token.
    pub async fn run(self: Arc<Self>) {
        let mut offset: Option<i64> = None;
        loop {
            let mut params = GetUpdatesParams::builder()
                .timeout(POLL_TIMEOUT_SECS)
                .allowed_updates(vec![AllowedUpdate::Message, AllowedUpdate::CallbackQuery])
                .build();
            params.offset = offset;

            match self.bot.get_updates(&params).await {
                Ok(response) => {
                    for update in response.result {
                        offset = Some(i64::from(update.update_id) + 1);
                        match update.content {
                            UpdateContent::Message(message) => {
                                let event = IncomingMessage {
                                    chat_id: message.chat.id,
                                    user_id: message.from.as_ref().map(|user| i64::from(user.id)),
                                    is_bot: message.from.as_ref().is_some_and(|user| user.is_bot),
                                    message_id: message.message_id,
                                    text: message.text.clone(),
                                };
                                let mut pending = self.pending.lock().await;
                                let consumed = match pending.as_ref() {
                                    Some(PendingReply::Text { chat_id, .. })
                                        if *chat_id == event.chat_id && !event.is_bot =>
                                    {
                                        event.text.as_deref().map(str::trim)
                                            .filter(|text| !text.is_empty())
                                            .map(str::to_owned)
                                    }
                                    _ => None,
                                };
                                if let Some(answer) = consumed {
                                    if let Some(PendingReply::Text { sender, .. }) = pending.take() {
                                        let _ = sender.send(answer);
                                    }
                                } else {
                                    let _ = self.events.send(event);
                                }
                            }
                            UpdateContent::CallbackQuery(query) => {
                                let chat_id = query.message.as_ref().and_then(|message| {
                                    match message {
                                        MaybeInaccessibleMessage::Message(message) => Some(message.chat.id),
                                        _ => None,
                                    }
                                });
                                let answer = match query.data.as_deref() {
                                    Some("yes") => Some(true),
                                    Some("no") => Some(false),
                                    _ => None,
                                };
                                let mut pending = self.pending.lock().await;
                                let consumed = match (pending.as_ref(), chat_id, answer) {
                                    (Some(PendingReply::YesNo { chat_id: expected, .. }), Some(actual), Some(value))
                                        if *expected == actual => Some(value),
                                    _ => None,
                                };
                                if let Some(value) = consumed {
                                    if let Some(PendingReply::YesNo { sender, .. }) = pending.take() {
                                        let _ = sender.send(value);
                                    }
                                    let ack = AnswerCallbackQueryParams::builder()
                                        .callback_query_id(query.id)
                                        .build();
                                    let _ = self.bot.answer_callback_query(&ack).await;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Err(error) => {
                    eprintln!("Telegram getUpdates failed: {error}");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
}

pub fn configured_chat_id() -> Result<i64, TelegramError> {
    std::env::var("TELEGRAM_CHAT_ID")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(TelegramError::MissingChatId)
}

pub fn default_timeout() -> Duration {
    std::env::var("TELEGRAM_ASK_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMEOUT_SECS))
}
