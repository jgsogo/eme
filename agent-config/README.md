# agent-config

Shared configuration-loading primitives for the AI agent platform.

This crate standardizes TOML file and directory loading and exposes structured,
path-aware errors through `ConfigError`.

## Responsibilities

- Read and deserialize a single TOML file.
- Load TOML files from a directory in deterministic path order.
- Preserve the source path and underlying I/O or TOML parsing error.
- Represent semantic validation failures, duplicate definitions, and empty
  configuration directories as typed errors.

## Boundaries

- Domain crates own their configuration structs and semantic validation rules.
- This crate does not know about agents, providers, tools, or runtime behavior.
- It does not read environment variables, instantiate clients, or register
  executable tools.

## API

- `read_toml<T>(path)`
- `read_toml_dir<T>(directory)`
- `ConfigError`

Directory loading ignores non-TOML entries and returns files in sorted order.
An empty directory returns an empty vector; the caller decides whether that is
valid for its configuration domain.
