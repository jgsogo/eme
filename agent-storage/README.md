# agent-storage

## Purpose

The `agent-storage` crate will provide the persistence layer for the AI agent platform. It will define how execution data is stored and retrieved while keeping storage concerns separate from agent execution and service orchestration.

The crate will establish storage contracts that allow `agent-service` to persist and inspect runs without depending directly on a particular database.

## Responsibilities

The crate will implement:

- **Run persistence:** storage and retrieval of run identity, input, status, timestamps, and final outcome.
- **Message persistence:** durable storage of the conversation associated with each run, including model and tool messages.
- **Event persistence:** ordered storage and retrieval of execution events for inspection, auditing, and event streaming.
- **Storage abstractions:** interfaces for the service layer to use without depending on a concrete database.
- **SQLite backend:** an initial local storage implementation suitable for development and single-instance deployments.
- **Recovery support:** the persistence primitives needed by the service layer to identify interrupted runs and, as the execution model evolves, support checkpointing and resumption.

## Boundaries

- `agent-storage` will own persistence contracts and concrete storage adapters.
- `agent-service` will decide when run state, messages, and events need to be persisted.
- `agent-runtime` will remain independent of database technology.
- `agent-server` will configure and compose the selected storage implementation.

## Non-goals

This crate will not schedule or execute agent runs, expose an HTTP API, or decide authorization policy. Persistence alone will not imply exactly-once execution of external tool side effects; recovery and idempotency semantics will need to be defined across the service and tool boundaries.
