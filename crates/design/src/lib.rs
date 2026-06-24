//! design —— Shepherd 设计/提案上下文(OpenSpec / BMAD 风格的「Design 阶段 + 审批门」)。
//!
//! 在「需求 → 拆分 → 派发 → 验证」链路里,本上下文插在**拆分之前**:
//! agent(或人)产出**设计稿** → 人**审批门**(批准 / 驳回带反馈)→ 驳回则进入**修订循环** →
//! 批准(终态)后方可拆分实现。一致性边界是 `Proposal` 聚合(状态机钉死合法流转)。
//!
//! 与 fleet 执行者协同:`POST /proposal/{id}/design` 既可人填,也可由 runtime 的 agent 回调填入,
//! 复用 delivery 的「agent 出站回调」模式,实现真正的人机协同设计评审。
pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
