//! Thin Telegram Bot API helper built on `frankenstein`.
//!
//! Credentials and the target chat come from the environment so they are not
//! model-controlled arguments:
//!
//! - `TELEGRAM_BOT_TOKEN` — bot token from @BotFather
//! - `TELEGRAM_CHAT_ID` — numeric chat id of the user to contact
//! - `TELEGRAM_ASK_TIMEOUT_SECS` — optional timeout for `tg_ask_user` (default 120)

use std::time::{Duration, Instant};

use frankenstein::AsyncTelegramApi;
use frankenstein::client_reqwest::Bot;
use frankenstein::methods::{AnswerCallbackQueryParams, GetUpdatesParams, SendMessageParams};
use frankenstein::types::{
    AllowedUpdate, InlineKeyboardButton, InlineKeyboardMarkup, Message, ReplyMarkup,
};
use frankenstein::updates::{MaybeInaccessibleMessage, UpdateContent};
use thiserror::Error;

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const POLL_TIMEOUT_SECS: u32 = 20;

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
}

pub struct TelegramClient {
    bot: Bot,
    chat_id: i64,
}

impl TelegramClient {
    pub fn from_env() -> Result<Self, TelegramError> {
        let token =
            std::env::var("TELEGRAM_BOT_TOKEN").map_err(|_| TelegramError::MissingToken)?;
        let chat_id = std::env::var("TELEGRAM_CHAT_ID")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or(TelegramError::MissingChatId)?;

        Ok(Self {
            bot: Bot::new(&token),
            chat_id,
        })
    }

    /// Send a plain notification message. Does not wait for a reply.
    pub async fn notify(&self, text: &str) -> Result<(), TelegramError> {
        let params = SendMessageParams::builder()
            .chat_id(self.chat_id)
            .text(text)
            .build();

        self.bot
            .send_message(&params)
            .await
            .map_err(|e| TelegramError::Api(e.to_string()))?;

        Ok(())
    }

    /// Ask a free-text question and wait for the next text reply from the chat.
    pub async fn ask_text(
        &self,
        question: &str,
        timeout: Duration,
    ) -> Result<String, TelegramError> {
        let params = SendMessageParams::builder()
            .chat_id(self.chat_id)
            .text(question)
            .build();

        let sent = self
            .bot
            .send_message(&params)
            .await
            .map_err(|e| TelegramError::Api(e.to_string()))?;

        let after_message_id = sent.result.message_id;
        self.wait_for_text_reply(after_message_id, timeout).await
    }

    /// Ask a yes/no question with inline buttons and wait for the callback.
    pub async fn ask_yes_no(
        &self,
        question: &str,
        timeout: Duration,
    ) -> Result<bool, TelegramError> {
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
        };

        let params = SendMessageParams::builder()
            .chat_id(self.chat_id)
            .text(question)
            .reply_markup(ReplyMarkup::InlineKeyboardMarkup(keyboard))
            .build();

        let sent = self
            .bot
            .send_message(&params)
            .await
            .map_err(|e| TelegramError::Api(e.to_string()))?;

        let message_id = sent.result.message_id;
        self.wait_for_yes_no(message_id, timeout).await
    }

    async fn wait_for_text_reply(
        &self,
        after_message_id: i32,
        timeout: Duration,
    ) -> Result<String, TelegramError> {
        let deadline = Instant::now() + timeout;
        let mut offset: Option<i64> = None;

        loop {
            if Instant::now() >= deadline {
                return Err(TelegramError::Timeout(timeout));
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            let poll_secs = (remaining.as_secs() as u32)
                .min(POLL_TIMEOUT_SECS)
                .max(1);

            let mut params = GetUpdatesParams::builder()
                .timeout(poll_secs)
                .allowed_updates(vec![AllowedUpdate::Message])
                .build();
            params.offset = offset;

            let updates = self
                .bot
                .get_updates(&params)
                .await
                .map_err(|e| TelegramError::Api(e.to_string()))?;

            for update in updates.result {
                offset = Some(i64::from(update.update_id) + 1);

                if let UpdateContent::Message(message) = update.content {
                    if !is_from_chat(&message, self.chat_id) {
                        continue;
                    }
                    if message.message_id <= after_message_id {
                        continue;
                    }
                    if let Some(text) = message.text {
                        let text = text.trim();
                        if !text.is_empty() {
                            return Ok(text.to_owned());
                        }
                    }
                }
            }
        }
    }

    async fn wait_for_yes_no(
        &self,
        question_message_id: i32,
        timeout: Duration,
    ) -> Result<bool, TelegramError> {
        let deadline = Instant::now() + timeout;
        let mut offset: Option<i64> = None;

        loop {
            if Instant::now() >= deadline {
                return Err(TelegramError::Timeout(timeout));
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            let poll_secs = (remaining.as_secs() as u32)
                .min(POLL_TIMEOUT_SECS)
                .max(1);

            let mut params = GetUpdatesParams::builder()
                .timeout(poll_secs)
                .allowed_updates(vec![AllowedUpdate::CallbackQuery])
                .build();
            params.offset = offset;

            let updates = self
                .bot
                .get_updates(&params)
                .await
                .map_err(|e| TelegramError::Api(e.to_string()))?;

            for update in updates.result {
                offset = Some(i64::from(update.update_id) + 1);

                if let UpdateContent::CallbackQuery(query) = update.content {
                    let matches_message = match &query.message {
                        Some(MaybeInaccessibleMessage::Message(m)) => {
                            m.message_id == question_message_id && is_from_chat(m, self.chat_id)
                        }
                        _ => false,
                    };
                    if !matches_message {
                        continue;
                    }

                    let data = query.data.as_deref().unwrap_or("");
                    let answer = match data {
                        "yes" => Some(true),
                        "no" => Some(false),
                        _ => None,
                    };

                    if let Some(value) = answer {
                        let ack = AnswerCallbackQueryParams::builder()
                            .callback_query_id(query.id)
                            .build();
                        let _ = self.bot.answer_callback_query(&ack).await;
                        return Ok(value);
                    }
                }
            }
        }
    }
}

fn is_from_chat(message: &Message, chat_id: i64) -> bool {
    message.chat.id == chat_id
}

pub fn default_timeout() -> Duration {
    std::env::var("TELEGRAM_ASK_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMEOUT_SECS))
}
