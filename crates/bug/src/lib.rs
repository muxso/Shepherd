//! ms-bug —— 阶段 4,缺陷管理
//!
//! 明星是**数据驱动的缺陷状态流图**:缺陷状态(StatusItem)与允许的
//! 流转(StatusFlow 的 from→to 边)都是按项目配置的数据,而非硬编码 if。
//! 这里把"能否从 A 流转到 B""A 能流转到哪些状态"提炼成图上的纯查询,
//! 用例驱动覆盖每条边。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
