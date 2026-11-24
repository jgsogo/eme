-- Link scheduled task definitions to runs without coupling TOML task storage
-- to the execution database. History survives deletion of a task definition.
CREATE TABLE IF NOT EXISTS task_runs (
    run_id TEXT PRIMARY KEY NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    task_id TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_runs_task_id ON task_runs(task_id);
