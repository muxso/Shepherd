//! 适配器:IO 实现(内存 / PG / HTTP)。
pub mod in_memory;

pub use in_memory::InMemoryCommentRepository;

#[cfg(feature = "pg")]
pub mod pg;

#[cfg(feature = "http")]
pub mod http;
