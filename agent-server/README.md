# agent-server

## Purpose

The `agent-server` crate will be the executable entry point for running the AI agent platform as a long-lived service.

It will assemble the platform components, load their configuration, manage application startup and shutdown, and expose agent-run operations to external clients through an HTTP API.

## Telegram bot

Set `TELEGRAM_BOT_TOKEN` to enable the optional long-polling bot and `TELEGRAM_ALLOWED_USERS` to a comma-separated allowlist of numeric Telegram user IDs. Text prompts are submitted through the shared `RunManager` to the configured agent (default: `assistant`); use `--telegram-agent <name>` to select another configured agent. The bot acknowledges queued runs and sends the final result. `/start` and `/help` show usage.

```bash
export TELEGRAM_BOT_TOKEN="..."
export TELEGRAM_ALLOWED_USERS="123456789"
cargo run -p agent-server
```

The `telegram-tool` crate provides the shared `TelegramGateway`. It is the sole owner of `getUpdates` and dispatches incoming messages to the bot adapter or an active `tg_ask_user` request. The bot and tools share the same gateway instance; do not create another gateway or poller for the same token.

## Responsibilities

The crate will implement:

- **Application composition:** construction and configuration of model integrations, tools, policies, storage, and the run service.
- **HTTP API:** endpoints for submitting runs, inspecting run status and results, requesting cancellation, and accessing run events.
- **Event streaming:** a client-facing mechanism for receiving run progress, initially intended to use Server-Sent Events (SSE).
- **Configuration and startup:** loading service configuration, validating dependencies, and initializing the application.
- **Graceful shutdown:** coordinating service shutdown and the handling of active runs.
- **Operational interface:** the executable and deployment-facing configuration for running the platform as a service.

## Boundaries

- `agent-server` will own the process entry point, transport layer, and application composition.
- `agent-service` will own run orchestration, scheduling, and execution lifecycle management.
- `agent-storage` will own persistence contracts and storage backends.
- `agent-runtime` will execute individual agent runs.
- `agent-models` will provide provider-independent model integrations.

## Non-goals

The server will not implement agent execution logic, tool authorization, scheduling algorithms, or database access directly. Those responsibilities will be delegated to the corresponding crates.


## Google account connections (OAuth)

Google Calendar account linking is optional. Configure all four variables to enable it:

- `GOOGLE_OAUTH_CLIENT_ID`: OAuth client ID.
- `GOOGLE_OAUTH_CLIENT_SECRET`: OAuth client secret.
- `GOOGLE_OAUTH_REDIRECT_URI`: registered callback URL, normally `https://your-host/integrations/google/oauth/callback`.
- `EME_CREDENTIAL_ENCRYPTION_KEY`: stable 32-byte encryption key encoded as exactly 64 hexadecimal characters.

Generate the encryption key with `openssl rand -hex 32`. Keep it in a deployment secret manager, separate from the SQLite database, and back it up securely. Losing or changing the key makes stored refresh tokens and pending PKCE verifiers undecryptable. Never put this key in TOML, source control, browser configuration, or logs.

When the variables are absent, Google integration endpoints return `503 Service Unavailable` and the rest of the server remains available. A partial configuration or malformed encryption key prevents startup.

Authenticated endpoints:

- `POST /integrations/google/oauth/start` with `{"access":"read_only"}` or `{"access":"read_write"}` returns an authorization URL. The caller should navigate the browser to that URL.
- `GET /integrations/google/oauth/callback` is the registered Google redirect URI. The existing EME browser session must still be present.
- `GET /integrations/google/connections` lists the current user's connected Google accounts without returning credentials.
- `DELETE /integrations/google/connections/{connection_id}` disables the EME connection first, then attempts to revoke the Google refresh token.

OAuth state and PKCE verifiers are persisted with a ten-minute expiry. State is hashed at rest, the verifier is encrypted, and callbacks are single-use and bound to the initiating EME user. Google account identity is resolved through the OpenID Connect user-info endpoint and an unverified email is rejected.

The current API slice connects accounts only. Calendar discovery/selection, per-calendar permissions, family sharing, and user-context calendar tool execution are subsequent implementation steps.
