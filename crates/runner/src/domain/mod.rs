//! 领域层:agent 注册模型 + 执行结果 + 执行历史(零 IO)。
pub mod agent;
pub mod execution;

pub use agent::{AgentError, DispatchTarget, NewRunnerAgent, RemoteResult, RunnerAgent};
pub use execution::ExecutionRecord;
