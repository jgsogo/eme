# agent-models

## Purpose

The `agent-models` crate will provide the provider-independent model integration layer for the AI agent platform. It will define the common conversation interface used by the runtime and translate that interface to the APIs and data formats of supported model providers.

The crate will also own the model and provider metadata used to configure and resolve model clients.

## Responsibilities

The crate will implement:

- **Conversation interface:** common request, response, message, tool-call, and token-usage types that will isolate the rest of the platform from provider-specific protocols.
- **Client abstraction:** an asynchronous interface through which the runtime will request model completions.
- **Provider adapters:** implementations that will translate between the common conversation interface and individual provider APIs.
- **Provider configuration:** loading and validating provider definitions, credentials configuration, endpoint information, and supported models from data files.
- **Model registry and resolution:** resolving a provider/model identifier into a configured conversation client.
- **Model capabilities:** declarative provider/model classifications describing features such as chat, reasoning, coding, image and video input, audio, tool calling, structured output, and other supported modalities. Capabilities are compatibility requirements, not standardized guarantees or measurements of model quality; in particular, `reasoning` does not imply a uniform reasoning level.
- **Provider error handling:** a common error model for configuration, transport, provider responses, and unsupported models.

## Boundaries

- `agent-models` will own provider-specific API integration and the provider-independent conversation contract.
- `agent-runtime` will consume the conversation abstraction and will own agent execution semantics.
- `agent-service` and `agent-server` will use the model configuration and resolution facilities when assembling runs.
- Provider and model definitions will remain data; provider protocol implementations will remain Rust code.

## Non-goals

This crate will not manage agent runs, maintain agent conversation state across turns, register or execute tools, make authorization decisions, or expose an application-facing HTTP API.
