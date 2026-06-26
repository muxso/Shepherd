pub mod in_memory;
pub use in_memory::InMemoryCaseRepository;

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "pg")]
pub use pg::PgCaseRepository;

#[cfg(feature = "http")]
pub mod http;
