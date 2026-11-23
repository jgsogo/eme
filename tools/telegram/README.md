# telegram-tool

## Purpose

The `telegram-tool` crate provides the platform's collection of tools for interacting with Telegram. It is intended for short human-in-the-loop interactions from agents: asking a question and waiting for the answer, and sending notifications.

The crate currently implements two tools:

- `tg_notify_user` — send a one-way notification
- `tg_ask_user` — ask a question and wait for the reply (free text or yes/no buttons)

The crate also exposes a shared `TelegramGateway`. The server owns one gateway per bot token and shares it with the inbound bot adapter and tools.

## Responsibilities

- **Tool implementations:** model-facing definitions and argument schemas for each Telegram capability.
- **Telegram Bot API integration:** one shared gateway authenticates with the bot token, owns the only update poller, and routes replies to waiting interactive requests or broadcasts normal messages.
- **Recipient resolution:** derive the default recipient from trusted conversation identity, or resolve an explicit family `recipient_id` against every trusted family roster belonging to the authenticated user.
- **Error reporting:** translate Telegram API and timeout failures into tool errors for the runtime.

## Configuration

| Environment variable | Required | Description |
|---|---|---|
| `TELEGRAM_BOT_TOKEN` | yes | Bot token from [@BotFather](https://t.me/BotFather) |
| `TELEGRAM_ASK_TIMEOUT_SECS` | no | How long `tg_ask_user` waits for a reply (default: 120) |

Each recipient must have already started a private conversation with the bot. The server's inbound adapter additionally requires `TELEGRAM_ALLOWED_USERS`.

## Tools

### `tg_notify_user`

Sends a message and returns immediately.

Arguments:

- `message` (string, required)
- `recipient` (string, optional): a `recipient_id` from any trusted family context belonging to the authenticated user; omitted means the conversation owner.

### `tg_ask_user`

Sends a question and blocks until the user answers or the timeout elapses.

Arguments:

- `question` (string, required)
- `kind` (string, optional): `"text"` (default) or `"yes_no"`
- `recipient` (string, optional): a `recipient_id` from the trusted family context; omitted means the conversation owner.

Returns:

- free text, when `kind = "text"`
- `"yes"` or `"no"`, when `kind = "yes_no"`

## Boundaries

- `telegram-tool` owns Telegram-specific integration code, the shared gateway, and the implementations of its tools.
- `agent-runtime` owns registration, argument validation, authorization, and dispatch.
- Telegram chat IDs are resolved by the host from authenticated identity records; the model never supplies raw Telegram IDs.
- Explicit recipients must match a member of at least one family the authenticated user currently belongs to and have a linked Telegram account. The model receives EME `recipient_id` values, never raw Telegram IDs; the host resolves the Telegram ID again at execution time.

## Non-goals

This crate does not own agent execution, scheduling, conversation management, or authorization decisions. The gateway serializes interactive requests so concurrent `tg_ask_user` calls cannot steal each other's replies. Messages not consumed by a pending interactive request are broadcast to subscribers.
