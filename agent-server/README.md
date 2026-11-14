# agent-server

## Purpose

The `agent-server` crate will be the executable entry point for running the AI agent platform as a long-lived service.

It will assemble the platform components, load their configuration, manage application startup and shutdown, and expose agent-run operations to external clients through an HTTP API.

## Responsibilities

The crate will implement:

- **Application composition:** construction and configuration of model integrations, tools, policies, storage, and the run service.
- **HTTP API:** endpoints for submitting runs, inspecting run status and results, requesting cancellation, and accessing run events.
- **Event streaming:** a client-facing mechanism for receiving run progress, initially intended to use Server-Sent Events (SSE).
- **Configuration and startup:** loading service configuration, validating dependencies, and initializing the application.
- **Graceful shutdown:** coordinating service shutdown and the handling of active runs.
- **Operational interface:** the executable and deployment-facing configuration for running the platform as a service.

## Boundaries

- `agent-server` will own the process entry point, transport layer, and application composition.
- `agent-service` will own run orchestration, scheduling, and execution lifecycle management.
- `agent-storage` will own persistence contracts and storage backends.
- `agent-runtime` will execute individual agent runs.
- `agent-models` will provide provider-independent model integrations.

## Non-goals

The server will not implement agent execution logic, tool authorization, scheduling algorithms, or database access directly. Those responsibilities will be delegated to the corresponding crates.
