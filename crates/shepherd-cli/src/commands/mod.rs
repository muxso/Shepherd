//! Per-group command modules.
//!
//! Each module owns one `clap` subcommand enum plus its `run` function. `main.rs`
//! only declares the top-level [`crate::Cmd`] enum and dispatches to these.

pub mod agent;
pub mod api;
pub mod apidef;
pub mod apikey;
pub mod auth;
pub mod bug;
pub mod case;
pub mod caseexec;
pub mod comment;
pub mod debug;
pub mod decomposition;
pub mod env;
pub mod fcase;
pub mod follow;
pub mod import;
pub mod llm;
pub mod mcp;
pub mod notice;
pub mod org;
pub mod perf;
pub mod pfile;
pub mod plan;
pub mod pool;
pub mod prd;
pub mod project;
pub mod proposal;
pub mod req;
pub mod role;
pub mod root;
pub mod runner;
pub mod scenario;
pub mod skill;
pub mod task;
pub mod user;
pub mod verify;
