# Google authentication for the web console

The web console uses [oauth2-proxy](https://oauth2-proxy.github.io/oauth2-proxy/) with Google's OAuth provider. The proxy owns the login flow and forwards the authenticated Google account ID to `agent-server`.

## 1. Create Google OAuth credentials

In Google Cloud Console, create an OAuth client ID of type **Web application**.

Add this authorized redirect URI:

```text
https://eme.example.com/oauth2/callback
```

Configure the OAuth consent screen as appropriate for the accounts that should be allowed to sign in.

## 2. Configure oauth2-proxy

Create a cookie secret (32 bytes, base64-encoded), then provide these environment variables to oauth2-proxy:

- `OAUTH2_PROXY_CLIENT_ID`
- `OAUTH2_PROXY_CLIENT_SECRET`
- `OAUTH2_PROXY_COOKIE_SECRET`

Example `oauth2-proxy.cfg`:

```toml
provider = "google"
client_id = "YOUR_GOOGLE_OAUTH_CLIENT_ID"
client_secret = "YOUR_GOOGLE_OAUTH_CLIENT_SECRET"
cookie_secret = "YOUR_BASE64_COOKIE_SECRET"

http_address = "127.0.0.1:4180"
upstreams = ["static://202"]
email_domains = ["*"]

reverse_proxy = true
set_xauthrequest = true
pass_user_headers = false
pass_basic_auth = false
cookie_secure = true
cookie_httponly = true
cookie_samesite = "lax"
```

For a personal deployment, `email_domains = ["*"]` allows any Google account. Replace it with an explicit domain or use an authenticated email allow-list if access should be restricted.

The Google provider exposes the Google account ID as the proxy username by default. Do not enable `prefer_email_to_user`: email addresses can change, while the Google account ID is the stable owner key used by EME.

## 3. Put a reverse proxy in front

Example Nginx configuration (TLS termination omitted):

```nginx
server {
    listen 443 ssl;
    server_name eme.example.com;

    location = /oauth2/auth {
        proxy_pass http://127.0.0.1:4180;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header X-Forwarded-Uri $request_uri;
    }

    location /oauth2/ {
        proxy_pass http://127.0.0.1:4180;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location / {
        auth_request /oauth2/auth;
        error_page 401 = /oauth2/sign_in;

        auth_request_set $eme_google_user $upstream_http_x_auth_request_user;
        proxy_set_header X-Auth-Request-User $eme_google_user;

        # Never forward a client-supplied identity header.
        proxy_pass_request_headers on;
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

The `auth_request` subrequest is made to oauth2-proxy, and Nginx copies its authenticated `X-Auth-Request-User` response header to the upstream request. Nginx must overwrite the incoming header, not append to it.

## 4. Protect the backend

Run `agent-server` on a loopback address or a private container network that is not reachable directly from the Internet. Only the reverse proxy should be able to connect to it. Otherwise a client could bypass oauth2-proxy and forge the identity header.

The conversations API rejects requests without `X-Auth-Request-User` and uses that value as the conversation owner. The browser never supplies an owner ID.

## Local development

For local development without oauth2-proxy, use a separate explicitly trusted local-only setup. Do not expose the unauthenticated development server on a public interface.
