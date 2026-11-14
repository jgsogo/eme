//! Orchestration and lifecycle management for concurrent agent runs.
//!
//! This crate will manage multiple independent executions built on
//! `agent-runtime`. It will own run submission, scheduling, worker
//! coordination, cancellation, supervision, and run-event routing.
//!
//! The service layer will not implement model interaction or tool execution;
//! those responsibilities will remain in `agent-runtime`. It will also remain
//! independent of HTTP transport and concrete storage backends.
