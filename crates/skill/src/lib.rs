//! Skill library context: project-level Skill (name/description/instructions/
//! includes references). SkillLibrary expands includes into a Composition — an
//! ordered, deduplicated behavior spec injected into the executor at task
//! dispatch. domain/application/ports do no IO; pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
