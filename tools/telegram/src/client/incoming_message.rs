#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub chat_id: i64,
    pub user_id: Option<i64>,
    pub is_bot: bool,
    pub message_id: i32,
    /// Message ID being replied to, if this is a Telegram reply.
    pub reply_to_message_id: Option<i32>,
    pub text: Option<String>,
}
