//! 应用层:需求用例编排。依赖 `ports` trait,不碰具体 IO。
pub mod create_requirement;
pub mod list_requirements;
pub mod requirement_admin;

pub use create_requirement::{CreateRequirementError, CreateRequirementUseCase};
pub use list_requirements::ListRequirementsUseCase;
pub use requirement_admin::{RequirementCmdError, RequirementService};
