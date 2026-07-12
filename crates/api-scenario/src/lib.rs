//! API scenario context: the ApiScenario aggregate, whose steps reference API cases
//! (CASE), inline requests (REQUEST), or control structures (Loop/If/Once/Timer).
//!
//! compile_scenario flattens the nested step tree into a linear PlanStep sequence
//! (depth cap guards against pathological nesting); results are recorded as
//! ScenarioExecution. domain/application/ports do no IO; pg/http adapters are
//! feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
