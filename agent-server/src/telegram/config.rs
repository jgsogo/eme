use std::collections::HashSet;

use anyhow::{Context, Result};

pub fn allowed_users_from_env() -> Result<HashSet<i64>> {
    let value = std::env::var("TELEGRAM_ALLOWED_USERS")
        .context("TELEGRAM_ALLOWED_USERS must be set when Telegram is enabled")?;
    let ids = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<i64>()
                .with_context(|| format!("invalid Telegram user id '{value}'"))
        })
        .collect::<Result<HashSet<_>>>()?;

    if ids.is_empty() {
        anyhow::bail!("TELEGRAM_ALLOWED_USERS must contain at least one user id");
    }

    Ok(ids)
}
