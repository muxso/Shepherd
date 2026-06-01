//! 领域层:AI Skill 与组合规则,零 IO。
pub mod skill;

pub use skill::{
    Composition, NewSkill, Skill, SkillError, SkillLibrary, MAX_NAME_LEN,
};
