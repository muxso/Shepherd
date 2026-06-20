//! 领域层:agent 注册模型 + 执行结果(零 IO)。
pub mod agent;

pub use agent::{AgentError, DispatchTarget, NewRunnerAgent, RemoteResult, RunnerAgent};
