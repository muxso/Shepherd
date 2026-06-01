//! skill —— Shepherd AI Skill 编排(定义、复用、组合)
//!
//! 一个 Skill 是可复用的 AI 行为规范(name + instructions),可经 `includes` 组合其它 skill。
//! 核心是 **compose**:把若干 skill 经 includes **传递展开**成一份有序(依赖在前)、去重、**防环**的
//! 指令集,注入到交付执行者的提示里——这就是"规范 AI 行为"。
//! 通过 `project_id` 字符串引用项目,不与其它上下文产生类型耦合。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
