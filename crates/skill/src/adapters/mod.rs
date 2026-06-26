pub mod in_memory;

pub use in_memory::InMemorySkillRepository;

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "http")]
pub mod http;
