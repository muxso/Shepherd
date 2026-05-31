//! 适配器层。`in_memory` 纯 test double 始终编译;`pg`/`http` feature 门控。
pub mod in_memory;

pub use in_memory::InMemoryProjectRepository;

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "http")]
pub mod http;
