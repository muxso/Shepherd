//! 适配器层。in_memory 纯 test double;pg/http feature 门控。
pub mod in_memory;
pub use in_memory::{InMemoryPlanRepository, InMemoryScheduleStore};
#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "pg")]
pub use pg::PgScheduleStore;
#[cfg(feature = "http")]
pub mod http;
