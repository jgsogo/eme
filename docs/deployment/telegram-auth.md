# Telegram authentication for the web console

The web console uses the configured Telegram bot as its identity provider. No public OAuth callback or inbound connection to `agent-server` is required: the browser polls the local server, while the user confirms the login by opening a Telegram deep link.

## Configuration

Set these environment variables when starting `agent-server`:

- `TELEGRAM_BOT_TOKEN`: existing bot token.
- `TELEGRAM_BOT_USERNAME`: bot username without the leading `@`, used to create `https://t.me/<bot>?start=...` links.
- `TELEGRAM_ALLOWED_USERS`: comma-separated numeric Telegram user IDs. Only these users can use the bot or authenticate to the web console.
- `WEB_COOKIE_SECURE`: set to `true` when the console is served over HTTPS. It defaults to `false` for private LAN/local HTTP development.

The Telegram bot must be enabled. The same allow-list applies to Telegram conversations and web sign-in.

## Login flow

1. The web console requests a random, one-time login challenge. It expires after five minutes.
2. The server returns a Telegram deep link containing the challenge.
3. The user opens the link in a private chat with the bot. The Telegram `from.id` is checked against `TELEGRAM_ALLOWED_USERS`.
4. The bot completes the challenge. The challenge can only be completed once and only from a private chat.
5. The browser polls the challenge endpoint. Once approved, the server sets an opaque `HttpOnly`, `SameSite=Lax` session cookie.
6. The session expires after 30 days. Signing out revokes it.

The browser never supplies a Telegram user ID. The server derives the canonical owner as `telegram:<user_id>` from the authenticated Telegram update.

## Network and deployment notes

The web console can remain private on a LAN or behind a VPN. Telegram authentication uses the bot's outbound Bot API connection; it does not require Telegram to call back into the web server.

When exposing the console beyond a trusted private network, use HTTPS and set `WEB_COOKIE_SECURE=true`. Do not expose the HTTP server directly to the public Internet without TLS and an appropriate network access policy.

For now, browser sessions are held in memory. Restarting `agent-server` invalidates existing sessions, but persisted conversations remain available after users sign in again.

## Conversation identity

Telegram and web conversations use the same owner key, `telegram:<user_id>`. The Telegram user ID is stable; usernames and display names are not used as identity keys.

## LAN family invitations

Set `LAN_BASE_URL` to the HTTP address that family members can reach on your home network, including the port when needed. The server accepts only private, loopback, or link-local IP addresses; hostnames and public addresses are rejected. Examples:

```sh
LAN_BASE_URL=http://192.168.1.20:8080
# IPv6 ULA is also supported:
LAN_BASE_URL=http://[fd00::20]:8080
```

When this variable is configured, creating a family invitation returns a `lan_url` alongside the one-time token. Share that URL only with the intended recipient. The link is a bearer credential: anyone who obtains it can attempt to accept the invitation until it expires or is revoked. Acceptance still requires signing in with the recipient's Telegram account and choosing their family relationship. The server validates and consumes the invitation using the same atomic, single-use workflow as Telegram invitations.

Use a trusted Wi-Fi network and a short invitation expiry. This LAN link uses HTTP, so it is not encrypted in transit; do not use it across untrusted networks. If you use HTTPS through a trusted reverse proxy, this IP-only HTTP configuration is not the appropriate public URL mechanism.

If `LAN_BASE_URL` is unset, invitation creation continues to return the one-time token but does not include a LAN URL.
