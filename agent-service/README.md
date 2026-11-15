# Agent service

Orchestration and lifecycle management for concurrent agent runs.

## Run lifecycle

The service accepts run submissions and assigns each run a stable identifier.
A bounded queue decouples submission from execution, while a worker pool
coordinates concurrent runs through the `RunExecutor` interface.

The service owns the lifecycle around an individual runtime execution:

- Track run status and lifecycle transitions.
- Publish run events and retain the execution report.
- Request cancellation and distinguish cancellation outcomes.
- Supervise execution and report successful or failed completion.
- Enforce configured queue and worker limits.

The service does not implement the agent conversation loop or tool execution;
those belong to `agent-runtime`. The `AgentRuntimeExecutor` adapter connects
the service to the runtime, resolving the configured agent and its dependencies
for each submitted run.

## Scheduled tasks

The service can load scheduled agent executions from TOML files in
`data/tasks/`. Each file selects an existing agent, provides the prompt to
execute, and defines either a recurring cron schedule or a one-time RFC3339
timestamp.

Recurring task example:

```toml
name = "daily-weather"
description = "Daily weather report"
agent = "assistant"
prompt = "Retrieve and summarize today's weather forecast for Salamanca."
schedule = "0 0 8 * * *"
timezone = "Europe/Madrid"
enabled = true
```

Cron expressions use six fields (seconds, minutes, hours, day of month, month,
day of week). The optional timezone defaults to UTC. One-time tasks use
`at = "2026-11-20T09:00:00+01:00"`; the timestamp must include its UTC offset
and cannot be combined with `schedule` or `timezone`.

Tasks are validated at startup. Disabled tasks are not scheduled, and expired
one-time tasks are ignored. Each due task is submitted through `RunManager`,
so normal execution concurrency and queue limits still apply. A full queue is
logged and the occurrence is skipped; missed recurring occurrences are not
replayed after a restart. Task definitions are reloaded only when the server
restarts.

## Boundaries

- `agent-service` owns run submission, scheduling, worker coordination,
  cancellation, supervision, and service-level run events.
- `agent-runtime` executes one agent run, including model interaction, tool
  dispatch, and authorization.
- `agent-server` composes the service with configuration, transports, and
  application startup and shutdown.
- Storage and durable recovery belong to the storage layer, not the in-memory
  service lifecycle.
