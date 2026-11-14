# telegram-tool

## Purpose

The `telegram-tool` crate provides the platform's collection of tools for interacting with Telegram. It is intended for short human-in-the-loop interactions from agents: asking a question and waiting for the answer, and sending notifications.

The crate currently implements two tools:

- `tg_notify_user` — send a one-way notification
- `tg_ask_user` — ask a question and wait for the reply (free text or yes/no buttons)

A full control bot for the agent platform is out of scope for this crate today; that can be layered on later without changing these tools.

## Responsibilities

- **Tool implementations:** model-facing definitions and argument schemas for each Telegram capability.
- **Telegram Bot API integration:** authenticate with a bot token, send messages, poll for replies and callback queries.
- **Configuration boundary:** read the bot token and target chat id from the environment so secrets are not model-controlled arguments.
- **Error reporting:** translate Telegram API and timeout failures into tool errors for the runtime.

## Configuration

| Environment variable | Required | Description |
|---|---|---|
| `TELEGRAM_BOT_TOKEN` | yes | Bot token from [@BotFather](https://t.me/BotFather) |
| `TELEGRAM_CHAT_ID` | yes | Numeric chat id of the user to contact |
| `TELEGRAM_ASK_TIMEOUT_SECS` | no | How long `tg_ask_user` waits for a reply (default: 120) |

The target user must have already started a conversation with the bot (or the bot must be allowed to message them).

## Tools

### `tg_notify_user`

Sends a message and returns immediately.

Arguments:

- `message` (string, required)

### `tg_ask_user`

Sends a question and blocks until the user answers or the timeout elapses.

Arguments:

- `question` (string, required)
- `kind` (string, optional): `"text"` (default) or `"yes_no"`

Returns:

- free text, when `kind = "text"`
- `"yes"` or `"no"`, when `kind = "yes_no"`

## Boundaries

- `telegram-tool` owns Telegram-specific integration code and the implementations of its tools.
- `agent-runtime` owns registration, argument validation, authorization, and dispatch.
- Credentials stay outside model-controlled arguments.

## Non-goals

This crate does not own agent execution, scheduling, conversation management, or authorization decisions. It is not a general-purpose Telegram bot framework; its public purpose is the set of Telegram capabilities the agent platform exposes as tools.
