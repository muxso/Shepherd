//! requirement —— Shepherd 需求管理(多版本)
//!
//! 复用六边形分层(domain / ports / application / adapters)。本上下文落地三条核心规则,
//! 用测试钉死:
//! 1. **同项目内需求标题唯一**,且唯一性**忽略已软删除**的需求(删后可重建同名);
//! 2. **线性多版本**:每次修订(revise)追加一个**不可变**版本快照(version 单调递增,
//!    历史版本不可改写);
//! 3. **baseline 指针**:基线显式指向某个已存在版本;修订**不自动移动基线**——
//!    可以在保留稳定基线的同时起草新版本,直到显式定基。
//!
//! 需求是 Shepherd "需求 → 任务拆分 → 交付 → 完整性验证" 链路的源头:验收标准
//! (acceptance_criteria)随版本快照,后续 task / verification 上下文据此拆分与验收。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
