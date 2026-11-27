# EME — AI Agent Platform

[![CI](https://github.com/jgsogo/eme/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jgsogo/eme/actions/workflows/ci.yml?query=branch%3Amain)

**Project:** [Landing page](https://jgsogo.es/eme/) · [Rust API documentation](https://jgsogo.es/eme/docs/rust/)

A Rust-based platform for building and running AI agents, with a focus on reliable execution, deterministic tool authorization, and provider-independent model integration.

The goal is to build more than an application that calls an LLM: a foundation for agents that can interact with external systems through controlled capabilities, execute multi-step workflows, and eventually recover from failures and resume work.

**Status:** Early development. The core agent runtime and model integrations are functional; the platform and its operational capabilities are evolving.

## Goals

The project explores the engineering foundations required to run useful and trustworthy AI agents:

- **Agent runtime:** Execute multi-step agent workflows, manage conversation state, dispatch tools, and control the execution lifecycle.
- **Provider independence:** Integrate different LLM providers behind a common conversation API.
- **Deterministic security:** Keep authorization and execution decisions outside the model. The model can request an action, but the runtime decides whether it is allowed.
- **Tool execution:** Expose external capabilities through typed tool definitions and JSON Schema validation.
- **Observability:** Record execution events to understand what an agent did, when it did it, and why.
- **Durability:** Evolve towards persistent execution, checkpoints, retries, and recovery after interruptions.

## Architecture

The platform is organized around a separation of responsibilities:

```text
                  Application / Client
                          |
                          v
                    Agent Runtime
                          |
             +------------+------------+
             |            |             |
             v            v             v
        Model Adapter  Tool Registry  Policy Engine
             |            |             |
             v            v             |
       Conversation    Tool Execution  |
          Client            |           |
             |              |           |
             v              v           v
         LLM Provider   External     Authorization
                        Services
             |
             v
        Model Response
             |
             v
        Agent Runtime
             |
             v
       Execution Events
```

### Main components

The workspace separates provider integration, individual agent execution, and tool integrations into dedicated crates. Service-level orchestration and transport are being introduced in separate crates. Each crate's README describes its purpose, responsibilities, boundaries, and non-goals.

#### `agent-models`

Provider-independent model integration.

- `ConversationClient` defines the common API for conversational model requests.
- `ConversationRequest` and `ConversationResponse` represent provider-independent messages, tool calls, and usage information.
- Provider implementations translate between the common conversation API and provider-specific APIs.
- A registry loads provider and model definitions from TOML files.

Current integrations:

- Google Gemini.
- OpenAI-compatible APIs: Cloudflare Workers AI, DeepSeek, Groq, and OpenRouter.

The catalogue advertises capabilities supported by the current adapter, not every feature available from the upstream model API. For example, image-input models are not marked as vision-capable until the shared conversation contract and provider adapters can actually send image content.

#### `agent-runtime`

The execution engine responsible for running agents.

Its current capabilities include:

- Agent configuration and execution limits.
- Multi-turn model interaction.
- Tool registration and dispatch.
- JSON Schema validation of tool arguments.
- Per-agent and global tool allow-list authorization.
- Tool execution and error propagation.
- Cancellation and execution timeouts.
- Runtime event collection.

The model proposes tool calls, but it does not execute them directly. The runtime validates and authorizes each call before dispatching it.

#### Tool integration crates

Tool crates implement capabilities that agents can invoke through the runtime. They own integrations with external systems where applicable; the runtime remains responsible for validating, authorizing, and dispatching calls.

##### `tools/common` (`common-tools`)

Generic tools backed by information available directly from the runtime environment. These tools do not need an external service or web search.

Current tool:

- `get_current_datetime`: returns the current UTC timestamp in RFC 3339 format, plus Unix timestamps in seconds and milliseconds.

##### `tools/meal-planner` (`meal-planner`)

The Meal Planner is an application module inside the EME monorepo. It owns its domain rules, application use cases and SQLite persistence as those features are introduced. The server initializes its database at startup and shares the store with HTTP handlers; the default path is `data/meal-planner.sqlite` (overridable with `--meal-planner-database`). It reuses the existing EME server and web console rather than introducing a separately deployed service or frontend. Agent tools will be added as thin adapters when the corresponding use cases are ready.

##### `tools/alexa` (`alexa-tool`)

The Alexa integration crate contains Alexa-related tools. It currently provides `alexa_speak`, which sends speech to an Alexa device.

Alexa-specific authentication, service interaction, and reusable device-resolution logic belong in this crate. Tool definitions expose individual operations to agents without making credentials or privileged configuration model-controlled.

## Planned service architecture

The following crates establish the structure for running the platform as a long-lived service. Their documentation describes intended responsibilities; these crates are scaffolding and do not implement these capabilities yet.

#### `agent-service`

The orchestration layer will manage multiple independent agent runs. It will provide run submission and lifecycle management, bounded scheduling, worker coordination, cancellation, execution supervision, and run-event routing. It will use `agent-runtime` to execute individual runs and will remain independent of HTTP transport and concrete storage backends.

#### `agent-storage`

The persistence layer will define storage contracts for runs, conversation messages, and execution events. It will provide concrete storage adapters, beginning with SQLite, and the persistence primitives needed for later recovery, checkpointing, and resumption.

#### `agent-server`

The executable service will compose the platform components and expose them to external clients. It will own application startup and shutdown, the HTTP API for submitting and managing runs, and client-facing event streaming. Run orchestration will belong to `agent-service`, while persistence will belong to `agent-storage`.

## Current capabilities

The `basic_agent` example demonstrates an end-to-end agent workflow:

1. Load provider and agent configuration; the agent declares required model capabilities.
2. Select a configured model that satisfies those capabilities and configure the agent's tools.
3. Receive a user request.
4. Ask the model to determine whether a tool is needed.
5. Validate and authorize the requested tool call.
6. Execute the tool and append its result to the conversation.
7. Continue the interaction until the agent completes.

The example includes an addition tool and demonstrates tool-failure recovery: a deprecated tool returns an error, the model receives that error, and it can select an alternative tool.

## Model routing and fallback

Agents declare the capabilities they require rather than pinning a provider/model identifier. For each model turn, the runtime asks the configured registry for all compatible models. Candidate order is deterministic, and model identifiers remain provider-qualified.

The router resolves each candidate lazily and records the routing lifecycle:

1. The compatible candidate set is discovered.
2. Each candidate attempt is recorded before its request.
3. A failed candidate records a structured category (rate limited, quota exceeded, transient, candidate unavailable, or permanent).
4. Retryable or unavailable candidates may lead to a fallback event naming the next candidate.
5. A successful candidate is recorded as selected and is attached to the model-response event.

Permanent request failures stop routing immediately. Attempts are bounded by the number of compatible candidates. Routing events are available through the run events API and in the web console's **Current run events** panel. Events contain provider-qualified model IDs and failure categories, not credentials or raw provider response bodies.

This is per model invocation: a multi-turn run can select a different compatible model on a later turn. Tool execution remains outside the routing boundary and is never replayed by model fallback.

## Getting started

### Prerequisites

- Rust toolchain.
- Cargo.
- API credentials for a supported model provider.

### Configure a provider

Provider configuration files are loaded from:

```text
data/
└── providers/
    ├── cloudflare.toml
    ├── deepseek.toml
    ├── gemini.toml
    ├── groq.toml
    └── openrouter.toml
```

For example, a Cloudflare provider configuration:

```toml
name = "cloudflare"
client = "openai-compatible"
api_key_env = "CLOUDFLARE_API_TOKEN"
account_id_env = "CLOUDFLARE_ACCOUNT_ID"
base_url = "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1"

[[models]]
name = "@cf/zai-org/glm-4.7-flash"
capabilities = ["chat", "reasoning", "tool_calling"]
```

Set the corresponding environment variables:

```bash
export CLOUDFLARE_API_TOKEN="..."
export CLOUDFLARE_ACCOUNT_ID="..."
```

### Run the basic agent

From the repository root:

```bash
cargo run -p agent-runtime --example basic_agent -- \
  "Ask Alexa to say Hello"
```

## Media discovery

EME includes an optional family-aware movie and TV discovery integration using TMDB, Watchmode and Trakt. See [Media discovery and family recommendations](docs/media.md) for provider configuration, Trakt linking, family profiles and recommendation behavior.

## Design principles

### The model is not the authority

An LLM can decide which tool it wants to use, but it cannot grant itself permissions.

Authorization is enforced by the runtime through deterministic policies. Tool arguments are validated against their declared schemas before execution.

### Tools are capabilities

Tools represent capabilities made available to an agent. They should be independently defined, registered, validated, authorized, and executed.

The runtime should not need to know whether a tool sends a message, queries an API, accesses a file, or interacts with a device. Related tools can share an integration crate when they depend on the same external system.

### Provider independence

The agent runtime should not depend on provider-specific conversation formats or APIs.

Provider-specific details belong in the model integration layer. The runtime operates on common conversation messages, tool definitions, and responses.

### Execution should be observable

Agent execution is a sequence of meaningful events, not a single opaque model invocation.

Events provide the foundation for debugging, auditing, monitoring, and eventually durable execution.

## Development

Install `pre-commit` and enable the repository hooks:

```bash
pre-commit install
pre-commit run --all-files
cargo build
cargo test
```

**The objective is not simply to make agents more capable. It is to make their execution controllable, inspectable, and reliable.**
