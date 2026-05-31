//! 领域层:项目的业务规则,零 IO。
pub mod project;

pub use project::{NewProject, Project, ProjectError};
