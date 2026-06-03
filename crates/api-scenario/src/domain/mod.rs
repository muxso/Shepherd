//! 领域层:场景 + 步骤 + 编译(打通执行),零 IO。
pub mod scenario;

pub use scenario::{
    flatten_step, ApiScenario, InlineRequest, NewApiScenario, NewScenarioStep, RefMode,
    RunnableStep, ScenarioError, ScenarioStatus, ScenarioStep, StepKind,
};
