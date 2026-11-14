# agent-runtime

## Purpose

The `agent-runtime` crate will provide the execution engine for an individual agent run. It will coordinate model interaction, conversation state, tool dispatch, authorization, and execution limits to carry a task through a multi-turn agent workflow.

It will remain the core execution component used by higher-level services, without depending on an HTTP transport or a concrete persistence backend.

## Responsibilities

The crate will implement:

- **Agent definition and configuration:** the data structures and loading mechanisms that will describe an agent's instructions, model reference, available tools, and execution limits.
- **Model adaptation:** the boundary that will adapt the provider-independent client interface from `agent-models` to the runtime's model request and response types.
- **Conversation loop:** the multi-turn execution process that will send conversation context to a model, interpret its response, dispatch requested tools, append tool results, and continue until a terminal condition is reached.
- **Tool contracts and dispatch:** interfaces for executable tools and dispatchers, together with registration and lookup of tool implementations. Tool integrations load metadata and bind implementations through `ToolRegistry::register_from_file`; direct registration remains available for programmatically constructed definitions.
- **Tool definitions and validation:** loading tool metadata and validating model-provided arguments against declared JSON Schemas before execution.
- **Deterministic authorization:** policy interfaces and implementations that will decide whether a requested tool action is permitted independently of model output.
- **Execution lifecycle controls:** run identity, iteration limits, timeout and cancellation handling, and explicit terminal outcomes.
- **Execution events and failures:** structured events and failure information that will make an individual run inspectable by service-level orchestration.

## Boundaries

- `agent-runtime` will execute one agent run at a time from the caller's perspective; managing many concurrent runs, queueing, and worker coordination will belong to `agent-service`.
- `agent-models` will own provider-specific model integrations; the runtime will consume its provider-independent interface.
- Tool implementations will live in dedicated tool crates and will be supplied to the runtime through its tool interfaces.
- `agent-storage` will own durable persistence; the runtime will not depend on a particular database.
- `agent-server` will own external transport and application startup.

## Non-goals

This crate will not provide a long-running multi-run service, an HTTP API, a distributed scheduler, a concrete database backend, or provider-specific networking code.
