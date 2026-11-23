-- Durable invocation-level accounting and immutable pricing history.
CREATE TABLE IF NOT EXISTS model_usage_records (
    invocation_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    principal_id TEXT,
    occurred_at TEXT NOT NULL,
    record_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_model_usage_run_time
    ON model_usage_records(run_id, occurred_at, invocation_id);
CREATE INDEX IF NOT EXISTS idx_model_usage_principal_time
    ON model_usage_records(principal_id, occurred_at);
CREATE INDEX IF NOT EXISTS idx_model_usage_provider_model_time
    ON model_usage_records(provider, model, occurred_at);

CREATE TABLE IF NOT EXISTS model_pricing (
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    version TEXT NOT NULL,
    effective_from TEXT NOT NULL,
    effective_until TEXT,
    pricing_json TEXT NOT NULL,
    PRIMARY KEY(provider, model, version)
);

CREATE INDEX IF NOT EXISTS idx_model_pricing_effective
    ON model_pricing(provider, model, effective_from, effective_until);
