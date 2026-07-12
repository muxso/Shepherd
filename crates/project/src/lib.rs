//! Business rule: project names are unique within an organization, and uniqueness ignores
//! soft-deleted projects (the same name can be recreated).

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
