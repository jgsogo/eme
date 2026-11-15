# Agent service

Orchestration and lifecycle management for concurrent agent runs.

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
