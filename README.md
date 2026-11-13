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
6. Execute the tool and append