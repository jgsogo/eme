//! Shared Telegram gateway.
//!
//! Exactly one instance owns Telegram's update stream. Tools and inbound
//! adapters subscribe to its dispatcher instead of calling getUpdates
//! independently.

use std::{collections::HashMap, sync::Arc, time::Duration};

use super::{IncomingMessage, PendingReply, TelegramError};

use frankenstein::{
    AsyncTelegramApi,
    client_reqwest::Bot,
    methods::{AnswerCallbackQueryParams, GetUpdatesParams, SendMessageParams},
    types::{
        AllowedUpdate, InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage,
        ReplyMarkup,
    },
    updates::UpdateContent,
};
use tokio::sync::{Mutex, broadcast, oneshot};

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const POLL_TIMEOUT_SECS: u32 = 20;
const EVENT_CAPACITY: usize = 256;

/// Owns the Bot API client and the sole getUpdates polling loop.
///
/// Create one gateway per bot token and share its Arc with tools and inbound
/// adapters. Interactive replies are correlated by the Telegram message ID
/// they reply to; all other messages are broadcast to normal message handlers.
pub struct TelegramGateway {
    bot: Bot,
    pending: Mutex<HashMap<(i64, i32), PendingReply>>,
    dispatch_lock: Mutex<()>,
    events: broadcast::Sender<IncomingMessage>,
}

impl TelegramGateway {
    pub fn new(token: &str) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Arc::new(Self {
            bot: Bot::new(token),
            pending: Mutex::new(HashMap::new()),
            dispatch_lock: Mutex::new(()),
            events,
        })
    }

    pub fn from_env() -> Result<Arc<Self>, TelegramError> {
        let token = std::env::var("TELEGRAM_BOT_TOKEN").map_err(|_| TelegramError::MissingToken)?;
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
        self.bot
            .send_message(&params)
            .await
            .map_err(|error| TelegramError::Api(error.to_string()))?;
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
        let (sender, receiver) = oneshot::channel();

        // Serialize registration with update dispatch. This prevents an
        // already-fetched update from being mistaken for the answer.
        let dispatch = self.dispatch_lock.lock().await;
        let sent = self
            .send_message(
                &SendMessageParams::builder()
                    .chat_id(chat_id)
                    .text(question)
                    .build(),
            )
            .await
            .map_err(|error| TelegramError::Api(error.to_string()))?;

        let key = (chat_id, sent.result.message_id);
        self.pending
            .lock()
            .await
            .insert(key, PendingReply::Text { sender });
        drop(dispatch);

        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(_)) => Err(TelegramError::DispatcherClosed),
            Err(_) => {
                self.pending.lock().await.remove(&key);
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
        let (sender, receiver) = oneshot::channel();
        let dispatch = self.dispatch_lock.lock().await;

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

        let sent = self
            .bot
            .send_message(&params)
            .await
            .map_err(|error| TelegramError::Api(error.to_string()))?;

        let key = (chat_id, sent.result.message_id);
        self.pending
            .lock()
            .await
            .insert(key, PendingReply::YesNo { sender });
        drop(dispatch);

        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(_)) => Err(TelegramError::DispatcherClosed),
            Err(_) => {
                self.pending.lock().await.remove(&key);
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
                        let _dispatch = self.dispatch_lock.lock().await;

                        match update.content {
                            UpdateContent::Message(message) => {
                                let event = IncomingMessage {
                                    chat_id: message.chat.id,
                                    user_id: message
                                        .from
                                        .as_ref()
                                        .and_then(|user| i64::try_from(user.id).ok()),
                                    is_bot: message.from.as_ref().is_some_and(|user| user.is_bot),
                                    message_id: message.message_id,
                                    reply_to_message_id: message
                                        .reply_to_message
                                        .as_ref()
                                        .map(|reply| reply.message_id),
                                    text: message.text.clone(),
                                };
                                let mut pending = self.pending.lock().await;
                                dispatch_message(event, &mut pending, &self.events);
                            }
                            UpdateContent::CallbackQuery(query) => {
                                let message_key =
                                    query.message.as_ref().and_then(|message| match message {
                                        MaybeInaccessibleMessage::Message(message) => {
                                            Some((message.chat.id, message.message_id))
                                        }
                                        _ => None,
                                    });
                                let answer = match query.data.as_deref() {
                                    Some("yes") => Some(true),
                                    Some("no") => Some(false),
                                    _ => None,
                                };
                                let mut pending = self.pending.lock().await;
                                pending.retain(|_, reply| match reply {
                                    PendingReply::Text { sender } => !sender.is_closed(),
                                    PendingReply::YesNo { sender } => !sender.is_closed(),
                                });

                                let consumed = match (message_key, answer) {
                                    (Some(key), Some(value)) => match pending.remove(&key) {
                                        Some(PendingReply::YesNo { sender }) => {
                                            let _ = sender.send(value);
                                            true
                                        }
                                        Some(reply @ PendingReply::Text { .. }) => {
                                            pending.insert(key, reply);
                                            false
                                        }
                                        None => false,
                                    },
                                    _ => false,
                                };

                                if consumed {
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

/// Route a normal Telegram message either to its matching interactive tool
/// call or to inbound subscribers. A reply is consumed only when it matches
/// the chat and message ID of a pending text question.
fn dispatch_message(
    event: IncomingMessage,
    pending: &mut HashMap<(i64, i32), PendingReply>,
    events: &broadcast::Sender<IncomingMessage>,
) {
    pending.retain(|_, reply| match reply {
        PendingReply::Text { sender } => !sender.is_closed(),
        PendingReply::YesNo { sender } => !sender.is_closed(),
    });

    let key = event
        .reply_to_message_id
        .map(|question_id| (event.chat_id, question_id));
    let answer = event
        .text
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned);

    let consumed = if !event.is_bot {
        match (key, answer) {
            (Some(key), Some(answer)) => match pending.remove(&key) {
                Some(PendingReply::Text { sender }) => {
                    // Even if the waiting task was cancelled concurrently,
                    // this is still a reply, never a fresh inbound prompt.
                    let _ = sender.send(answer);
                    true
                }
                Some(reply @ PendingReply::YesNo { .. }) => {
                    // A text reply is not a yes/no button response. Keep the
                    // interaction pending and let the inbound adapter decide
                    // how to handle this unmatched reply.
                    pending.insert(key, reply);
                    false
                }
                None => false,
            },
            _ => false,
        }
    } else {
        false
    };

    if !consumed {
        let _ = events.send(event);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn incoming(reply_to_message_id: Option<i32>) -> IncomingMessage {
        IncomingMessage {
            chat_id: 42,
            user_id: Some(7),
            is_bot: false,
            message_id: 200,
            reply_to_message_id,
            text: Some("  the answer  ".to_owned()),
        }
    }

    #[tokio::test]
    async fn matching_reply_delivers_exact_text_and_is_not_broadcast() {
        let (events, _) = broadcast::channel(4);
        let mut subscriber = events.subscribe();
        let (sender, receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::Text { sender });

        dispatch_message(incoming(Some(100)), &mut pending, &events);

        assert_eq!(receiver.await.unwrap(), "the answer");
        assert!(matches!(
            subscriber.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert!(pending.is_empty());
    }

    #[test]
    fn reply_to_an_unrelated_message_does_not_satisfy_pending_question() {
        let (events, _) = broadcast::channel(4);
        let mut subscriber = events.subscribe();
        let (sender, mut receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::Text { sender });

        dispatch_message(incoming(Some(999)), &mut pending, &events);

        assert!(receiver.try_recv().is_err());
        assert!(pending.contains_key(&(42, 100)));
        assert_eq!(subscriber.try_recv().unwrap().reply_to_message_id, Some(999));
    }

    #[test]
    fn ordinary_message_is_broadcast_without_consuming_pending_question() {
        let (events, _) = broadcast::channel(4);
        let mut subscriber = events.subscribe();
        let (sender, mut receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::Text { sender });

        dispatch_message(incoming(None), &mut pending, &events);

        assert!(receiver.try_recv().is_err());
        assert!(pending.contains_key(&(42, 100)));
        assert_eq!(subscriber.try_recv().unwrap().reply_to_message_id, None);
    }

    #[test]
    fn reply_from_another_chat_does_not_satisfy_pending_question() {
        let (events, _) = broadcast::channel(4);
        let mut subscriber = events.subscribe();
        let (sender, mut receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::Text { sender });
        let mut event = incoming(Some(100));
        event.chat_id = 43;

        dispatch_message(event, &mut pending, &events);

        assert!(receiver.try_recv().is_err());
        assert!(pending.contains_key(&(42, 100)));
        assert_eq!(subscriber.try_recv().unwrap().chat_id, 43);
    }
}
