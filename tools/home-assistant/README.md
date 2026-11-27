# Home Assistant tool

This crate integrates EME agents with the Home Assistant REST API.

## Configuration

Set these environment variables for the EME server:

- HOME_ASSISTANT_URL: Home Assistant base URL, for example http://homeassistant.local:8123.
- HOME_ASSISTANT_TOKEN: a Home Assistant long-lived access token.

The token is sent as an HTTP Bearer token and is never placed in a URL or returned to the model.

## Tools

| Tool | Purpose | Access |
| --- | --- | --- |
| home_assistant.get_states | List current entity states | Read |
| home_assistant.get_state | Read one entity | Read |
| home_assistant.get_services | Discover available services | Read |
| home_assistant.get_history | Read entity history | Read |
| home_assistant.get_config | Read core configuration | Read |
| home_assistant.call_service | Invoke a service | Control |

Each operation is registered independently. Grant home_assistant.call_service only to agents permitted to control devices; the runtime's existing per-agent tool allow-list is the authorization boundary.

The integration uses the REST API only. Event subscriptions and real-time updates over the Home Assistant WebSocket API are out of scope for this first version.
