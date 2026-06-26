pub mod create_environment;
pub mod delete_environment;
pub mod get_environment;
pub mod list_environments;
pub mod update_environment;

pub use create_environment::{CreateEnvironmentError, CreateEnvironmentUseCase};
pub use delete_environment::{DeleteEnvironmentError, DeleteEnvironmentUseCase};
pub use get_environment::{GetEnvironmentError, GetEnvironmentUseCase};
pub use list_environments::{ListEnvironmentsError, ListEnvironmentsUseCase};
pub use update_environment::{UpdateEnvironmentError, UpdateEnvironmentUseCase};

use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct EnvironmentInput {
    pub project_id: String,
    pub name: String,
    pub base_url: String,
    pub headers: Vec<(String, String)>,
    pub variables: BTreeMap<String, String>,
    pub enabled: bool,
}
