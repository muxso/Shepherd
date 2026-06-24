//! follow —— 关注人(通用)
//!
//! MeterSphere 里「关注人」分散重复在缺陷 / 需求 / 用例等各模块里;这里把它提炼成一个
//! **跨实体的通用能力**:任何业务对象都能被 `(projectId, entityType, entityId)` 唯一定位,
//! 用户对其「关注 / 取消关注」,系统据此给出「某对象的关注人列表」与「我关注的对象」。
//!
//! 关注是**个人动作**(非 RBAC 资源),幂等:重复关注不报错、不产生重复记录。
//! 领域模型刻意极薄——价值在于「让关注成为一等概念」而非散落的布尔列。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
