//! system-setting —— 用户 / 权限上下文(单 crate,六边形分层为模块)。
//!
//! - `domain` / `ports` / `application`:纯逻辑,零 IO 依赖。
//! - `adapters`:`in_memory`/`directory` 是 test double(纯);`pg`(feature=pg)、
//!   `http`(feature=http)是真实 IO 适配器,仅在对应 feature 开启时编译。
//!
//! 默认 `cargo build` 不启用任何 IO feature,sqlx/axum 不在依赖图里 ——
//! 因此 domain/application 即便手滑也 import 不到它们(编译期保证)。
//! feature 开启后,由 `tests/architecture.rs` 静态扫描禁止 domain 模块引用 IO crate。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
