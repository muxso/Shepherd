//! 应用层:skill 用例编排。
pub mod create_skill;
pub mod skill_admin;

pub use create_skill::{CreateSkillError, CreateSkillUseCase};
pub use skill_admin::{SkillCmdError, SkillService};
