//! Telegram long-polling adapter. Prompts use the shared RunManager.
use std::{collections::HashSet, time::Duration};
use agent_service::{RunId, RunManager, RunStatus, SubmitRunError};
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

pub fn allowed_users_from_env() -> Result<HashSet<i64>> {
    let value = std::env::var("TELEGRAM_ALLOWED_USERS")
        .context("TELEGRAM_ALLOWED_USERS must be set when Telegram is enabled")?;
    let ids = value.split(',').map(str::trim).filter(|s| !s.is_empty())
        .map(|s| s.parse::<i64>().with_context(|| format!("invalid Telegram user id '{s}'")))
        .collect::<Result<HashSet<_>>>()?;
    if ids.is_empty() { return Err(anyhow!("TELEGRAM_ALLOWED_USERS must contain at least one user id")); }
    Ok(ids)
}
#[derive(Deserialize)]
struct Api<T> { ok: bool, result: Option<T>, description: Option<String> }
#[derive(Deserialize)]
struct Update { update_id: i64, message: Option<Message> }
#[derive(Deserialize)]
struct Message { chat: Chat, from: Option<User>, text: Option<String> }
#[derive(Deserialize)]
struct Chat { id: i64 }
#[derive(Deserialize)]
struct User { id: i64, is_bot: bool }
fn url(token: &str, method: &str) -> String { format!("https://api.telegram.org/bot{token}/{method}") }

async fn send(client: &Client, token: &str, chat: i64, text: &str) -> Result<()> {
    let response = client.post(url(token, "sendMessage"))
        .json(&json!({"chat_id":chat,"text":text})).send().await?.error_for_status()?;
    let api = response.json::<Api<serde_json::Value>>().await?;
    if !api.ok { return Err(anyhow!("Telegram sendMessage: {}", api.description.unwrap_or_default())); }
    Ok(())
}
async fn poll(client: &Client, token: &str, offset: Option<i64>) -> Result<Vec<Update>> {
    let mut params = json!({"timeout":25,"allowed_updates":["message"]});
    if let Some(offset) = offset { params["offset"] = json!(offset); }
    let response = client.post(url(token, "getUpdates")).json(&params).send().await?.error_for_status()?;
    let api = response.json::<Api<Vec<Update>>>().await?;
    if !api.ok { return Err(anyhow!("Telegram getUpdates: {}", api.description.unwrap_or_default())); }
    Ok(api.result.unwrap_or_default())
}
fn truncate(s: &str) -> String {
    if s.chars().count() <= 3500 { return s.to_owned(); }
    format!("{}\n\n[Response truncated]", s.chars().take(3500).collect::<String>())
}
async fn report(client: Client, token: String, chat: i64, manager: RunManager, id: RunId) {
    loop {
        match manager.get(id).await {
            Some(run) if run.status.is_terminal() => {
                let text = match run.status {
                    RunStatus::Completed => run.response.unwrap_or_else(|| "Completed without a text response.".into()),
                    RunStatus::Failed => format!("Execution failed: {}", run.error.unwrap_or_default()),
                    RunStatus::Cancelled => "Execution cancelled.".into(),
                    RunStatus::TimedOut => "Execution timed out.".into(),
                    RunStatus::MaxIterationsReached => "Maximum iterations reached.".into(),
                    _ => unreachable!(),
                };
                if let Err(e) = send(&client, &token, chat, &truncate(&text)).await { eprintln!("Telegram result delivery failed: {e:#}"); }
                return;
            }
            Some(_) => tokio::time::sleep(Duration::from_millis(500)).await,
            None => return,
        }
    }
}
pub async fn run_bot(token: String, allowed: HashSet<i64>, agent: String, manager: RunManager) {
    let client = match Client::builder().timeout(Duration::from_secs(35)).build() {
        Ok(c) => c, Err(e) => { eprintln!("Telegram HTTP client: {e}"); return; }
    };
    let mut offset = None;
    loop {
        match poll(&client, &token, offset).await {
            Ok(updates) => for update in updates {
                offset = Some(update.update_id + 1);
                let Some(message) = update.message else { continue };
                let Some(user) = message.from else { continue };
                if user.is_bot { continue; }
                let Some(prompt) = message.text.map(|s| s.trim().to_owned()) else { continue };
                if prompt.is_empty() { continue; }
                if !allowed.contains(&user.id) {
                    let _ = send(&client, &token, message.chat.id, "You are not authorized to use this bot.").await;
                    continue;
                }
                if prompt == "/start" || prompt == "/help" {
                    let _ = send(&client, &token, message.chat.id, &format!("Send a prompt to run the '{agent}' agent.")).await;
                    continue;
                }
                match manager.submit(agent.clone(), prompt).await {
                    Ok(id) => {
                        let _ = send(&client, &token, message.chat.id, &format!("Run {id} queued.")).await;
                        tokio::spawn(report(client.clone(), token.clone(), message.chat.id, manager.clone(), id));
                    }
                    Err(SubmitRunError::QueueFull) => {
                        let _ = send(&client, &token, message.chat.id, "The agent queue is full; try again shortly.").await;
                    }
                }
            },
            Err(e) => { eprintln!("Telegram polling error: {e:#}"); tokio::time::sleep(Duration::from_secs(2)).await; }
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn truncates_long_text() {
        let text = "x".repeat(4000);
        assert!(super::truncate(&text).ends_with("[Response truncated]"));
    }
}
