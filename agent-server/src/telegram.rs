//! Telegram inbound adapter. It consumes events from the shared gateway;
//! it never polls Telegram directly.

mod commands;
mod config;
mod formatting;
mod message_handler;
mod report;

#[cfg(test)]
mod tests;

pub use config::allowed_users_from_env;

use std::{collections::HashSet, sync::Arc};

use agent_server::auth::TelegramAuth;
use agent_service::{FamilyService, RunManager};
use agent_storage::{ConversationStore, FileUserIdentityStore, SqliteFamilyStore};
use telegram_tool::{IncomingMessage, TelegramGateway};
use tokio::sync::{Mutex, broadcast};

use message_handler::handle_message;

pub async fn run_bot(
    gateway: Arc<TelegramGateway>,
    allowed: HashSet<i64>,
    agent: String,
    agents: Vec<String>,
    manager: RunManager,
    store: Arc<ConversationStore>,
    identities: Arc<FileUserIdentityStore>,
    families: Arc<SqliteFamilyStore>,
    auth: Arc<TelegramAuth>,
    mut updates: broadcast::Receiver<IncomingMessage>,
) {
    let active_conversations = Arc::new(Mutex::new(HashSet::new()));
    loop {
        match updates.recv().await {
            Ok(message) => {
                handle_message(
                    &gateway,
                    &allowed,
                    &agent,
                    &agents,
                    &manager,
                    &store,
                    &identities,
                    &families,
                    &active_conversations,
                    &auth,
                    message,
                )
                .await;
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                eprintln!("Telegram dispatcher lagged; skipped {skipped} messages");
            }
            Err(broadcast::error::RecvError::Closed) => {
                eprintln!("Telegram update dispatcher closed");
                return;
            }
        }
    }
}
