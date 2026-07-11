//! 技能库上下文:项目级 Skill(名称/说明/指令 instructions/includes 引用),
//! SkillLibrary 按 includes 展开组合(Composition)成一份有序去重的行为规范,供任务派发时注入执行者。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
