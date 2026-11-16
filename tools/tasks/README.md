# Task management tools

The `task_list`, `task_add`, and `task_remove` tools manage scheduled work for the
authenticated user. Ownership is taken exclusively from the trusted
`ToolContext.user_id`; it is not an argument accepted from the model.

Tasks are stored as individual TOML files in `data/tasks/<uuid>.toml`. A task
contains a globally unique UUID, its mandatory `user_id`, a per-user unique
name, the agent and prompt to execute, and exactly one scheduling mode:

- `schedule`: a cron expression with seconds; optionally provide an IANA
  `timezone` (UTC is the default).
- `at`: a one-time RFC3339 timestamp with an explicit UTC offset.

For example:

```toml
id = "f3a8e2c1-ff2c-4a67-9e37-c9f0c9c3d01a"
user_id = "telegram:118716073"
name = "daily-briefing"
description = "Prepare the daily briefing"
agent = "assistant"
prompt = "Prepare my daily briefing."
schedule = "0 0 8 * * *"
timezone = "Europe/Madrid"
enabled = true
```

At startup, task files predating ownership and UUID filenames are migrated
once: they are assigned to `telegram:118716073`, assigned a UUID, and rewritten
to the canonical UUID filename. The same registry instance is shared by the
tools and scheduler, so additions and removals take effect without restarting
the server. Scheduled runs receive the task owner's identity in their
`ToolContext`.
