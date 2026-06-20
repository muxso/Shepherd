//! 领域层:agent 注册模型 + 用例规格 + 执行结果 + 执行历史(零 IO)。
pub mod agent;
pub mod case_spec;
pub mod execution;

pub use agent::{
    AgentError, AgentTarget, DispatchTarget, NewRunnerAgent, RemoteResult, RunnerAgent,
};
pub use case_spec::CaseSpec;
pub use execution::ExecutionRecord;
