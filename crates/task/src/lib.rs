//! task —— Shepherd 任务拆分(需求 → 可独立交付的任务 DAG)
//!
//! 一致性边界是 **Decomposition 聚合**:一个需求版本对应一张任务图。聚合内钉死三条规则:
//! 1. **依赖必须指向已存在的任务**(增量构造 → 结构上无环,DAG by construction);
//! 2. **就绪 = 依赖全部 Verified**:一个任务只有在其所有前置依赖都验证通过后才可派发——
//!    这正是"每个任务可独立交付"在 DAG 上的精确含义(根任务先就绪,完成后解锁下游);
//! 3. **状态机流转**:Pending→Dispatched(就绪门控)→Running→Delivered→Verified;
//!    任意执行/验证失败 → Failed,可重试回 Pending。
//!
//! 任务的执行者(Claude Code / Codex)与完整性验证分别归 delivery / verification 上下文;
//! 本上下文只持有"拆成什么、依赖谁、现在到哪一步",通过 `requirement_id` + 版本号(字符串/整数)
//! 引用需求,不与 requirement crate 产生类型耦合。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
