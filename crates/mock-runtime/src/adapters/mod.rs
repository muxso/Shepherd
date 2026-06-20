//! 适配器层。in_memory 始终编译;axum 兜底路由 feature 门控。
pub mod in_memory;
pub use in_memory::InMemoryRuleSource;

#[cfg(feature = "http")]
pub mod http;
