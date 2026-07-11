//! 用例评审上下文:ReviewRecord 评审记录与状态机(未评审/评审中/通过/不通过/重新评审),
//! PassRule 决定单人或多人评审的通过判定;ReviewSetting 为项目级评审配置。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
