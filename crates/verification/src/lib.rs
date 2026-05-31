//! verification —— Shepherd 完整性验证(需求 ←→ 任务 ←→ 实现 双向追溯 + 缺口检测)
//!
//! 闭合"牧羊人"主链路的最后一环。一个需求版本对应一个 `Verification` 聚合,持有该版本的
//! 验收标准(快照)与**覆盖链**(每条标准 ↔ 哪些任务实现它)。把多条覆盖链聚合成每条标准的
//! 状态,再聚合成整体**完整性报告**(复用 case-review 的"多裁决 → 一状态"聚合精神)。
//!
//! 双向追溯:
//! - **需求 → 任务**:每条验收标准应被至少一个任务覆盖;无覆盖 = 前向缺口(Uncovered);
//! - **任务 → 实现**:覆盖任务须已交付且验证(satisfied);仅覆盖未验证 = 后向缺口(Unverified)。
//!
//! 通过 `requirement_id`/版本 与 `decomposition_id`/`task_id`(字符串)引用其它上下文,无类型耦合。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
