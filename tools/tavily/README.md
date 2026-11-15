# Tavily tool

Tavily-backed web search integration for EME. The crate exposes the
`web_search` tool and keeps provider-specific HTTP details out of the agent
runtime.

## Configuration

Set the API key in the environment before starting the agent server:

```bash
export TAVILY_API_KEY="..."
```

The tool sends authenticated JSON requests to `POST https://api.tavily.com/search`.
It uses Tavily's `basic` search depth by default to keep latency and API credit
usage predictable.

## Tool: `web_search`

Required argument:

- `query`: concise search query (1-400 characters).

Optional arguments:

- `max_results`: number of results (1-20, default 5).
- `search_depth`: `ultra-fast`, `fast`, `basic`, or `advanced` (default `basic`).
- `topic`: `general`, `news`, or `finance` (default `general`).
- `time_range`: `day`, `week`, `month`, or `year`.
- `include_domains` / `exclude_domains`: domain filters.

The response contains the original query, result titles, URLs, extracted
snippets, relevance scores, and publication dates when available. Raw page
content and Tavily-generated answers are intentionally disabled to keep tool
responses focused; agents can use source URLs in a subsequent fetch capability.

## Development

Run the crate tests with:

```bash
cargo test -p tavily-tool
```
