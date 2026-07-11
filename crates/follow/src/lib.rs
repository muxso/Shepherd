//! 关注关系上下文:用户对任意资源(缺陷/需求/用例等)的 Follow 关系存取,
//! FollowStore 端口承载增删查,供各模块复用「关注人」能力。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
