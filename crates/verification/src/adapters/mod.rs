pub mod in_memory;

pub use in_memory::InMemoryVerificationRepository;

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "http")]
pub mod http;
