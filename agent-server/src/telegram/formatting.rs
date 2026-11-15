use telegram_tool::TelegramGateway;

pub(super) async fn send_error(
    gateway: &TelegramGateway,
    chat_id: i64,
    action: &str,
    error: impl std::fmt::Display,
) {
    let _ = gateway
        .send_message(chat_id, &format!("Could not {action}: {error}"))
        .await;
}

pub(super) fn truncate(text: &str) -> String {
    if text.chars().count() <= 3500 {
        return text.to_owned();
    }

    format!(
        "{}\n\n[Response truncated]",
        text.chars().take(3500).collect::<String>()
    )
}
