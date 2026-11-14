# agent-service

## Purpose

The `agent-service` crate will provide the orchestration layer for running multiple independent agent executions as a long-lived service.

It will sit between service interfaces (such as the HTTP API provided by `agent-server`) and the single-run execution engine in `agent-runtime`. Its responsibility will be to manage runs as independent units of work, rather than to implement model interaction or tool execution itself.

## Responsibilities

The crate will implement:

- **Run management:** creation of run identities, submission, lookup, and lifecycle transitions.
- **Scheduling and admission control:** bounded queues, worker coordination, and configurable concurrency limits.
- **Execution supervision:** starting agent runs through `agent-runtime`, tracking their progress, and collecting their outcomes.
- **Cancellation and deadlines:** coordinating cancellation requests and execution limits for individual runs.
- **Run events:** routing execution events to the appropriate run and making them available to storage and subscribers.
- **Service-level coordination:** connecting runtime execution with persistence without coupling the runtime to a particular database or transport.

## Boundaries

- `agent-runtime` will remain responsible for executing one agent run, including model interaction, tool dispatch, and policy enforcement.
- `agent-service` will manage many runs and their lifecycle. It will not own HTTP routing or a concrete persistence implementation.
- `agent-storage` will provide the persistence contracts and storage implementations used by the service.
- `agent-server` will expose service operations to external clients.

## Non-goals

This crate will not implement provider-specific model clients, individual tools, HTTP endpoints, or database-specific queries. Those responsibilities will remain in their respective crates.
