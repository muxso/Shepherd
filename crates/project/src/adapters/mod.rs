pub mod in_memory;
pub mod in_memory_member;
pub mod in_memory_template;

pub use in_memory::InMemoryProjectRepository;
pub use in_memory_member::InMemoryProjectMemberRepository;
pub use in_memory_template::InMemoryTemplateRepository;

#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "http")]
pub mod member_http;
#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "pg")]
pub mod pg_member;
#[cfg(feature = "pg")]
pub mod pg_template;
#[cfg(feature = "http")]
pub mod template_http;
