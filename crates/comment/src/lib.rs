//! 通用评论上下文:Comment 以 (target_type, target_id) 挂到任意资源(缺陷/需求/用例等),
//! 增删查由 CommentRepository 端口承载。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
