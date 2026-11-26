//! Shared Telegram gateway.
//!
//! Exactly one instance owns Telegram's update stream. Tools and inbound
//! adapters subscribe to its dispatcher instead of calling getUpdates
//! independently.

use std::{collections::HashMap, sync::Arc, time::Duration};

use super::{IncomingMessage, PendingReply, TelegramError, markdown::markdown_to_telegram_html};

use frankenstein::{
    AsyncTelegramApi, ParseMode,
    client_reqwest::Bot,
    methods::{
        AnswerCallbackQueryParams, GetUpdatesParams, SendMessageParams, SetMessageReactionParams,
    },
    types::{
        AllowedUpdate, InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage,
        ReactionType, ReactionTypeEmoji, ReplyMarkup,
    },
    updates::UpdateContent,
};
use tokio::sync::{Mutex, broadcast, oneshot};

const DEFAULT_TIMEOUT_SECS: u64 = 300;
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

/// Add a concise marker to questions that require a user response.
fn format_question(question: &str) -> String {
    format!(
        "❓ Question\n\n{question}\n\n↩️ To answer, use Telegram’s Reply action on this message. New messages or standalone comments will not be accepted."
    )
}

/// Distinguish one-way notifications from questions that require a response.
fn format_notification(message: &str) -> String {
    format!("🔔 Notification\n\n{message}")
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
        let rendered = markdown_to_telegram_html(text);
        let params = SendMessageParams::builder()
            .chat_id(chat_id)
            .text(rendered)
            .parse_mode(ParseMode::Html)
            .build();
        self.bot
            .send_message(&params)
            .await
            .map_err(|error| TelegramError::Api(error.to_string()))?;
        Ok(())
    }

    pub async fn send_message_with_buttons(
        &self,
        chat_id: i64,
        text: &str,
        rows: Vec<Vec<(String, String)>>,
    ) -> Result<(), TelegramError> {
        let keyboard = InlineKeyboardMarkup {
            inline_keyboard: rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|(label, data)| {
                            InlineKeyboardButton::builder()
                                .text(label)
                                .callback_data(data)
                                .build()
                        })
                        .collect()
                })
                .collect(),
            force_reply: None,
        };
        let params = SendMessageParams::builder()
            .chat_id(chat_id)
            .text(markdown_to_telegram_html(text))
            .parse_mode(ParseMode::Html)
            .reply_markup(ReplyMarkup::InlineKeyboardMarkup(keyboard))
            .build();
        self.bot
            .send_message(&params)
            .await
            .map_err(|error| TelegramError::Api(error.to_string()))?;
        Ok(())
    }

    pub async fn react_to_message(
        &self,
        chat_id: i64,
        message_id: i32,
        emoji: &str,
    ) -> Result<(), TelegramError> {
        let reaction = ReactionType::Emoji(ReactionTypeEmoji::builder().emoji(emoji).build());
        let params = SetMessageReactionParams::builder()
            .chat_id(chat_id)
            .message_id(message_id)
            .reaction(vec![reaction])
            .build();
        self.bot
            .set_message_reaction(&params)
            .await
            .map_err(|error| TelegramError::Api(error.to_string()))?;
        Ok(())
    }

    pub async fn notify_user(&self, chat_id: i64, text: &str) -> Result<(), TelegramError> {
        self.send_message(chat_id, &format_notification(text)).await
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
        let rendered = markdown_to_telegram_html(&format_question(question));
        let params = SendMessageParams::builder()
            .chat_id(chat_id)
            .text(rendered)
            .parse_mode(ParseMode::Html)
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
        let question = format!(
            "{}\n\nOr use one of the Yes/No buttons below.",
            format_question(question)
        );
        let rendered = markdown_to_telegram_html(&question);
        let params = SendMessageParams::builder()
            .chat_id(chat_id)
            .text(rendered)
            .parse_mode(ParseMode::Html)
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

    pub async fn ask_choice(
        &self,
        chat_id: i64,
        question: &str,
        options: Vec<(String, String)>,
        timeout: Duration,
    ) -> Result<String, TelegramError> {
        if options.is_empty() {
            return Err(TelegramError::Api("Cannot ask a choice question without options".to_owned()));
        }
        let (sender, receiver) = oneshot::channel();
        let dispatch = self.dispatch_lock.lock().await;
        let keyboard = InlineKeyboardMarkup {
            inline_keyboard: options
                .into_iter()
                .map(|(label, value)| {
                    vec![InlineKeyboardButton::builder()
                        .text(label)
                        .callback_data(format!("choice:{value}"))
                        .build()]
                })
                .collect(),
            force_reply: None,
        };
        let params = SendMessageParams::builder()
            .chat_id(chat_id)
            .text(markdown_to_telegram_html(&format_question(question)))
            .parse_mode(ParseMode::Html)
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
            .insert(key, PendingReply::Choice { sender });
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
                                    first_name: message
                                        .from
                                        .as_ref()
                                        .map(|user| user.first_name.clone()),
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
                                let choice = query
                                    .data
                                    .as_deref()
                                    .and_then(|data| data.strip_prefix("choice:"));
                                let mut pending = self.pending.lock().await;
                                let consumed = dispatch_callback(message_key, answer, &mut pending)
                                    || dispatch_choice(message_key, choice, &mut pending);

                                drop(pending);
                                let ack = AnswerCallbackQueryParams::builder()
                                    .callback_query_id(query.id)
                                    .build();
                                let _ = self.bot.answer_callback_query(&ack).await;

                                if !consumed
                                    && !matches!(query.data.as_deref(), Some("yes" | "no"))
                                    && !query.data.as_deref().is_some_and(|data| data.starts_with("choice:"))
                                {
                                    if let (Some((chat_id, message_id)), Some(data)) =
                                        (message_key, query.data)
                                    {
                                        let event = IncomingMessage {
                                            chat_id,
                                            user_id: i64::try_from(query.from.id).ok(),
                                            first_name: Some(query.from.first_name),
                                            is_bot: query.from.is_bot,
                                            message_id,
                                            reply_to_message_id: None,
                                            text: Some(data),
                                        };
                                        let _ = self.events.send(event);
                                    }
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
        PendingReply::Choice { sender } => !sender.is_closed(),
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
                Some(PendingReply::Choice { sender }) => {
                    let _ = sender.send(answer);
                    true
                }
                Some(PendingReply::YesNo { sender }) => {
                    if let Some(answer) = parse_yes_no(&answer) {
                        let _ = sender.send(answer);
                        true
                    } else {
                        // Keep waiting for an explicit yes/no reply to this
                        // question. The inbound adapter ignores this reply.
                        pending.insert(key, PendingReply::YesNo { sender });
                        false
                    }
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

/// Route an inline-button response to the matching yes/no interaction.
/// Telegram associates callback queries with the message containing the buttons,
/// so this is equivalent to an explicit reply to that question.
fn dispatch_callback(
    message_key: Option<(i64, i32)>,
    answer: Option<bool>,
    pending: &mut HashMap<(i64, i32), PendingReply>,
) -> bool {
    pending.retain(|_, reply| match reply {
        PendingReply::Text { sender } => !sender.is_closed(),
        PendingReply::YesNo { sender } => !sender.is_closed(),
        PendingReply::Choice { sender } => !sender.is_closed(),
    });

    match (message_key, answer) {
        (Some(key), Some(value)) => match pending.remove(&key) {
            Some(PendingReply::YesNo { sender }) => {
                let _ = sender.send(value);
                true
            }
            Some(reply @ PendingReply::Text { .. })
            | Some(reply @ PendingReply::Choice { .. }) => {
                pending.insert(key, reply);
                false
            }
            None => false,
        },
        _ => false,
    }
}


/// Route an inline-button response to the matching finite-choice question.
fn dispatch_choice(
    message_key: Option<(i64, i32)>,
    answer: Option<&str>,
    pending: &mut HashMap<(i64, i32), PendingReply>,
) -> bool {
    match (message_key, answer) {
        (Some(key), Some(value)) => match pending.remove(&key) {
            Some(PendingReply::Choice { sender }) => {
                let _ = sender.send(value.to_owned());
                true
            }
            Some(reply) => {
                pending.insert(key, reply);
                false
            }
            None => false,
        },
        _ => false,
    }
}

/// Parse an explicit yes/no text answer, accepting common Spanish variants too.
fn parse_yes_no(answer: &str) -> Option<bool> {
    match answer.trim().to_lowercase().as_str() {
        "yes" | "y" | "sí" | "si" => Some(true),
        "no" | "n" => Some(false),
        _ => None,
    }
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
            first_name: Some("Javier".to_owned()),
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
        assert_eq!(
            subscriber.try_recv().unwrap().reply_to_message_id,
            Some(999)
        );
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

    #[tokio::test]
    async fn matching_yes_no_reply_delivers_answer() {
        let (events, _) = broadcast::channel(4);
        let (sender, receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::YesNo { sender });
        let mut event = incoming(Some(100));
        event.text = Some(" YES ".to_owned());

        dispatch_message(event, &mut pending, &events);

        assert!(receiver.await.unwrap());
        assert!(pending.is_empty());
    }

    #[test]
    fn invalid_yes_no_reply_keeps_question_pending() {
        let (events, _) = broadcast::channel(4);
        let mut subscriber = events.subscribe();
        let (sender, mut receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::YesNo { sender });
        let mut event = incoming(Some(100));
        event.text = Some("maybe".to_owned());

        dispatch_message(event, &mut pending, &events);

        assert!(receiver.try_recv().is_err());
        assert!(pending.contains_key(&(42, 100)));
        assert_eq!(
            subscriber.try_recv().unwrap().reply_to_message_id,
            Some(100)
        );
    }

    #[tokio::test]
    async fn callback_for_matching_yes_no_message_delivers_answer() {
        let (sender, receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::YesNo { sender });

        assert!(dispatch_callback(Some((42, 100)), Some(true), &mut pending));
        assert!(receiver.await.unwrap());
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn callback_for_matching_choice_message_delivers_selection() {
        let (sender, receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::Choice { sender });

        assert!(dispatch_choice(Some((42, 100)), Some("2"), &mut pending));
        assert_eq!(receiver.await.unwrap(), "2");
        assert!(pending.is_empty());
    }

    #[test]
    fn callback_for_wrong_message_does_not_consume_pending_question() {
        let (sender, mut receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::YesNo { sender });

        assert!(!dispatch_callback(
            Some((42, 999)),
            Some(false),
            &mut pending
        ));
        assert!(receiver.try_recv().is_err());
        assert!(pending.contains_key(&(42, 100)));
    }

    #[test]
    fn callback_does_not_consume_text_question_or_unknown_button_data() {
        let (sender, mut receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::Text { sender });

        assert!(!dispatch_callback(
            Some((42, 100)),
            Some(true),
            &mut pending
        ));
        assert!(!dispatch_callback(Some((42, 100)), None, &mut pending));
        assert!(receiver.try_recv().is_err());
        assert!(pending.contains_key(&(42, 100)));
    }

    #[tokio::test]
    async fn concurrent_questions_in_one_chat_are_matched_out_of_order() {
        let (events, _) = broadcast::channel(4);
        let mut subscriber = events.subscribe();
        let (first_sender, first_receiver) = oneshot::channel();
        let (second_sender, second_receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert(
            (42, 100),
            PendingReply::Text {
                sender: first_sender,
            },
        );
        pending.insert(
            (42, 101),
            PendingReply::Text {
                sender: second_sender,
            },
        );

        dispatch_message(incoming(Some(101)), &mut pending, &events);
        dispatch_message(incoming(Some(100)), &mut pending, &events);

        assert_eq!(second_receiver.await.unwrap(), "the answer");
        assert_eq!(first_receiver.await.unwrap(), "the answer");
        assert!(pending.is_empty());
        assert!(matches!(
            subscriber.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn late_reply_after_waiter_has_timed_out_is_not_delivered_or_dropped() {
        let (events, _) = broadcast::channel(4);
        let mut subscriber = events.subscribe();
        let (sender, receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::Text { sender });
        drop(receiver);

        dispatch_message(incoming(Some(100)), &mut pending, &events);

        assert!(pending.is_empty());
        assert_eq!(
            subscriber.try_recv().unwrap().reply_to_message_id,
            Some(100)
        );
    }

    #[test]
    fn text_reply_does_not_resolve_yes_no_question() {
        let (events, _) = broadcast::channel(4);
        let mut subscriber = events.subscribe();
        let (sender, mut receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::YesNo { sender });

        dispatch_message(incoming(Some(100)), &mut pending, &events);

        assert!(receiver.try_recv().is_err());
        assert!(pending.contains_key(&(42, 100)));
        assert_eq!(
            subscriber.try_recv().unwrap().reply_to_message_id,
            Some(100)
        );
    }

    #[test]
    fn empty_or_non_text_reply_does_not_resolve_text_question() {
        for text in [Some(String::new()), None] {
            let (events, _) = broadcast::channel(4);
            let mut subscriber = events.subscribe();
            let (sender, mut receiver) = oneshot::channel();
            let mut pending = HashMap::new();
            pending.insert((42, 100), PendingReply::Text { sender });
            let mut event = incoming(Some(100));
            event.text = text;

            dispatch_message(event, &mut pending, &events);

            assert!(receiver.try_recv().is_err());
            assert!(pending.contains_key(&(42, 100)));
            assert_eq!(
                subscriber.try_recv().unwrap().reply_to_message_id,
                Some(100)
            );
        }
    }

    #[test]
    fn bot_reply_does_not_resolve_text_question() {
        let (events, _) = broadcast::channel(4);
        let mut subscriber = events.subscribe();
        let (sender, mut receiver) = oneshot::channel();
        let mut pending = HashMap::new();
        pending.insert((42, 100), PendingReply::Text { sender });
        let mut event = incoming(Some(100));
        event.is_bot = true;

        dispatch_message(event, &mut pending, &events);

        assert!(receiver.try_recv().is_err());
        assert!(pending.contains_key(&(42, 100)));
        assert!(subscriber.try_recv().unwrap().is_bot);
    }

    #[test]
    fn questions_have_a_concise_question_marker() {
        assert_eq!(
            format_question("Which option should I use?"),
            "❓ Question\n\nWhich option should I use?\n\n↩️ To answer, use Telegram’s Reply action on this message. New messages or standalone comments will not be accepted."
        );
    }

    #[test]
    fn yes_no_parser_accepts_english_and_spanish_answers() {
        assert_eq!(parse_yes_no("YES"), Some(true));
        assert_eq!(parse_yes_no("sí"), Some(true));
        assert_eq!(parse_yes_no("si"), Some(true));
        assert_eq!(parse_yes_no("No"), Some(false));
        assert_eq!(parse_yes_no("maybe"), None);
    }

    #[test]
    fn notifications_have_a_distinct_notification_marker() {
        assert_eq!(
            format_notification("The task has completed."),
            "🔔 Notification\n\nThe task has completed."
        );
    }
}
