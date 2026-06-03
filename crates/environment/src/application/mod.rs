//! 应用层:环境 CRUD 用例编排。
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

/// 创建/更新环境的应用层输入(DTO,未校验)。domain `NewEnvironment::new` 负责校验。
#[derive(Debug, Clone)]
pub struct EnvironmentInput {
    pub project_id: String,
    pub name: String,
    pub base_url: String,
    pub headers: Vec<(String, String)>,
    pub variables: BTreeMap<String, String>,
    pub enabled: bool,
}
