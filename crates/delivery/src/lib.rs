//! Delivery context: dispatch tasks to executors and track delivery.
//!
//! Core pieces: the DeliveryAttempt state machine and Deliverable artifacts;
//! AgentExecutor abstracts the executor (stub / local / exec-http / exec-queue fleet);
//! WorkQueue + FleetRegistry support pull-style claiming and heartbeat registration
//! for agents without inbound network access; ExecutionEvent records execution progress.
//! domain/application/ports do no IO; pg/http/redis adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
