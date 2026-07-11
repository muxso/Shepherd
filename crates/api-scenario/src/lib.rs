//! 接口场景上下文:ApiScenario 聚合,步骤可引用接口用例(CASE)、内联请求(REQUEST)或控制结构(Loop/If/Once/Timer)。
//! compile_scenario 把嵌套步骤树展开为线性 PlanStep 序列(深度上限防病态嵌套);执行结果记为 ScenarioExecution。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
