# agent-models

## Purpose

The `agent-models` crate provides the provider-independent model integration layer for the AI agent platform. It defines the common conversation interface used by the runtime and translates that interface to the APIs and data formats of supported model providers.

The crate also owns the static provider/model catalogue and the model registry that resolves provider/model identifiers into configured conversation clients.

## Responsibilities

The crate implements:

- **Conversation interface:** common request, response, message, tool-call, and token-usage types that isolate the rest of the platform from provider-specific protocols.
- **Client abstraction:** an asynchronous interface through which the runtime requests model completions.
- **Provider adapters:** implementations that translate between the common conversation interface and individual provider APIs.
- **Provider configuration:** loading and validating provider definitions, credentials configuration, endpoint information, and supported models from data files.
- **Model registry and resolution:** resolving a provider/model identifier into a configured conversation client.
- **Model metadata:** declarative provider/model enablement, commercial access classification, and capabilities such as chat, reasoning, coding, image and video input, audio, tool calling, and structured output. Capabilities are compatibility requirements, not standardized guarantees or measurements of model quality.
- **Provider error handling:** a common error model for configuration, transport, provider responses, and unsupported or administratively disabled models.

A capability is advertised only when the current provider adapter can express it through the shared conversation API. Upstream model support alone is not sufficient: for example, a model may accept images or structured-output parameters at the provider API, while the current adapter exposes neither in its request contract.

## Configuration ownership

Provider and model definitions in `data/providers/*.toml` are the version-controlled catalogue. They describe technical integration details and default metadata. Administrative enablement overrides will be persisted separately; the TOML files remain the source for the provider/model definitions themselves.

The `access_tier` field is descriptive metadata (`free`, `free_tier`, `trial`, `paid`, `enterprise`, or `unknown`). Enterprise access generally requires an enterprise agreement or account and is unlikely to be available to personal deployments. The field is not a token price and is not used to calculate invocation costs. Versioned token pricing and cost calculations belong to `agent-metering`.

OpenRouter's free catalogue changes frequently. The TOML snapshot includes the free chat endpoints listed in November 2026 that can be invoked through the current OpenAI-compatible conversation adapter, plus the `openrouter/free` dynamic router. Image-generation, embedding, reranking, audio, and decision-specific endpoints are excluded until EME has matching request/response contracts. Free endpoints may have provider-specific rate limits and data-use terms; do not send confidential data unless the endpoint's terms have been reviewed.

## Boundaries

- `agent-models` owns provider-specific API integration and the provider-independent conversation contract.
- `agent-runtime` consumes the conversation abstraction and owns agent execution semantics.
- `agent-service` and `agent-server` use model configuration and resolution facilities when assembling runs.
- Provider and model definitions remain data; provider protocol implementations remain Rust code.

## Non-goals

This crate does not manage agent runs, maintain agent conversation state across turns, register or execute tools, make authorization decisions, or expose an application-facing HTTP API.
