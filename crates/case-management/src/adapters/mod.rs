//! 适配器层。in_memory 始终编译;pg/http feature 门控。
pub mod in_memory;
pub use in_memory::InMemoryCaseRepository;

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "pg")]
pub use pg::PgCaseRepository;

#[cfg(feature = "http")]
pub mod http;
