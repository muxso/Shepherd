//! 适配器层。in_memory 纯 test double;pg/http feature 门控。
pub mod in_memory;
pub mod in_memory_import_schedule;
pub use in_memory::InMemoryApiDefinitionRepository;
pub use in_memory_import_schedule::InMemoryImportScheduleStore;
#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "pg")]
pub mod pg_import_schedule;
#[cfg(feature = "pg")]
pub use pg_import_schedule::PgImportScheduleStore;
#[cfg(feature = "http")]
pub mod http;
