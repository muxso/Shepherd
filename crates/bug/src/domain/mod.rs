//! 领域层:缺陷 + 状态流图,零 IO。
pub mod bug;
pub mod status_flow;

pub use bug::{Bug, BugError, NewBug};
pub use status_flow::{StatusFlowGraph, StatusItem};
