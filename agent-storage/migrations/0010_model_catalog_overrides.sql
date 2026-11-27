-- Sparse administrative overrides for the version-controlled provider/model catalogue.
CREATE TABLE IF NOT EXISTS provider_catalog_overrides (
    provider TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER CHECK(enabled IS NULL OR enabled IN (0, 1)),
    disabled_reason TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS model_catalog_overrides (
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    enabled INTEGER CHECK(enabled IS NULL OR enabled IN (0, 1)),
    disabled_reason TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(provider, model)
);
