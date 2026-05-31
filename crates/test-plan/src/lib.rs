//! ms-testplan —— 阶段 5,测试计划
//!
//! 这是第三类领域逻辑(前有 case 的聚合投票、bug 的转移图):**执行统计聚合**。
//! 把 Java `TestPlanStatisticsResponse.calculate*` 与 `calculateStatusByChildren` 那串
//! 散在 DTO/Service 里的算术 + 状态推导,提炼成零 IO 的纯函数:
//! - 计划状态推导(未执行/进行中/已完成)
//! - 通过率 / 执行率 / 是否达阈值
//! - 计划组状态由子计划推导
//! - 计划组不可嵌套(GROUP 的 groupId 必须是根)

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
