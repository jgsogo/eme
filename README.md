# EME — AI Agent Platform

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

#### `agent-models`

Provider-independent model integration.

- `ConversationClient` defines the common API for conversational model requests.
- `ConversationRequest` and `ConversationResponse` represent provider-independent messages, tool calls, and usage information.
- Provider implementations translate between the common conversation API and provider-specific APIs.
- A registry loads provider and model definitions from TOML files.

Current integrations:

- Google Gemini.
- OpenAI-compatible APIs, including Cloudflare Workers AI.

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

## Current capabilities

The `basic_agent` example demonstrates an end-to-end agent workflow:

1. Load a provider registry and resolve a model.
2. Configure an agent and its available tools.
3. Receive a user request.
4. Ask the model to determine whether a tool is needed.
5. Validate and authorize the requested tool call.
6. Execute the tool and append its result to the conversation.
7. Continue the interaction until the agent completes.

The example includes an addition tool and demonstrates tool-failure recovery: a deprecated tool returns an error, the model receives that error, and it can select an alternative tool.

### Verified integrations

| Provider | Model | Result |
|---|---|---|
| Google Gemini | Gemini 3.5 Flash Lite | End-to-end tool calling |
| Cloudflare Workers AI | GLM-4.7-Flash | End-to-end tool calling and failure recovery |

These are development tests, not a claim of compatibility with every model or provider.

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
    ├── gemini.toml
    └── cloudflare.toml
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
capabilities = ["chat", "tool_calling"]
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
  cloudflare/@cf/zai-org/glm-4.7-flash \
  ./data
```

The example accepts:

```text
basic_agent <provider/model> [data-dir]
```

Alternatively, the model and data directory can be provided through:

- `AGENT_MODEL`
- `AGENT_DATA_DIR`

The example runs an agent that answers an arithmetic question using a registered tool.

## Design principles

### The model is not the authority

An LLM can decide which tool it wants to use, but it cannot grant itself permissions.

Authorization is enforced by the runtime through deterministic policies. Tool arguments are validated against their declared schemas before execution.

### Tools are capabilities

Tools represent capabilities made available to an agent. They should be independently defined, registered, validated, authorized, and executed.

The runtime should not need to know whether a tool sends a message, queries an API, accesses a file, or interacts with a device.

### Provider independence

The agent runtime should not depend on provider-specific conversation formats or APIs.

Provider-specific details belong in the model integration layer. The runtime operates on common conversation messages, tool definitions, and responses.

### Execution should be observable

Agent execution is a sequence of meaningful events, not a single opaque model invocation.

Events provide the foundation for debugging, auditing, monitoring, and eventually durable execution.

## Roadmap

The project is under active development. Areas being explored include:

- [x] Provider-independent conversation API.
- [x] Gemini integration.
- [x] OpenAI-compatible model integration.
- [x] Agent runtime and multi-turn execution.
- [x] Tool registration and dispatch.
- [x] JSON Schema argument validation.
- [x] Deterministic tool allow-list authorization.
- [x] Tool error propagation and recovery.
- [x] Cancellation and execution timeouts.
- [x] Runtime event collection.
- [ ] More comprehensive authorization policies and permission boundaries.
- [ ] Human approval for sensitive or irreversible actions.
- [ ] More external tool integrations.
- [ ] HTTP API for submitting and managing agent runs.
- [ ] Persistent execution state and event storage.
- [ ] Checkpoints, retries, and recovery after process restarts.
- [ ] Deployment and operational observability.

## Development

Install [pre-commit](https://pre-commit.com/) and enable the repository hooks:

```bash
pre-commit install
```

Run all hooks against the repository manually:

```bash
pre-commit run --all-files
```

Build the workspace:

```bash
cargo build
```

Run the tests:

```bash
cargo test
```

Run the basic agent:

```bash
cargo run -p agent-runtime --example basic_agent -- \
  <provider/model> \
  ./data
```

## Project direction

The long-term ambition is to provide a foundation for agents that can do useful work in real environments without giving up control over execution.

That means treating model interaction, tool execution, authorization, state management, and observability as distinct engineering concerns.

**The objective is not simply to make agents more capable. It is to make their execution controllable, inspectable, and reliable.**
