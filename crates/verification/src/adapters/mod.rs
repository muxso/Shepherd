pub mod in_memory;

pub use in_memory::InMemoryVerificationRepository;

#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "pg")]
pub mod pg;
