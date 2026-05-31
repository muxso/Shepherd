//! 领域层:需求的业务规则,零 IO。
pub mod requirement;

pub use requirement::{
    parse_criteria, AcceptanceCriterion, NewRequirement, Requirement, RequirementError,
    RequirementStatus, RequirementVersion, MAX_TITLE_LEN,
};
