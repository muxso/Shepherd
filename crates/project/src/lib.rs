//! project —— 阶段 2,项目管理
//!
//! 复用阶段 1 的六边形分层(domain / ports / application / adapters)。
//! 本模块聚焦两条 MeterSphere 真实业务规则,用测试钉死:
//! 1. 同一组织内项目名唯一;
//! 2. 唯一性**忽略已软删除**的项目——删掉后可以重建同名项目。
//!
//! 列表查询复用 `kernel` 的分页,串起阶段 0 的共享内核。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
