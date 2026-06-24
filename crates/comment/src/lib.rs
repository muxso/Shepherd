//! comment —— 通用评论。
//!
//! 评论是**多态**的:一条评论用 `(target_type, target_id)` 指向任意业务实体
//! (缺陷 / 需求 / 用例 / 评审 …),不与任何单一模块耦合。这样新实体想要评论区,
//! 不必各自重做一套 CRUD,只要复用本 crate 的端口 + HTTP 适配器即可。
//!
//! 分层照旧:domain/ports/application 保持纯净(无 IO),sqlx/axum 只落在 adapters。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
