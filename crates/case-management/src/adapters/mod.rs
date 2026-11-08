pub mod in_memory;
pub use in_memory::InMemoryCaseRepository;

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "pg")]
pub use pg::PgCaseRepository;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "http")]
pub mod test_assets_http;
#[cfg(feature = "pg")]
pub mod test_assets_pg;

#[cfg(feature = "pg")]
mod test_assets_import;
