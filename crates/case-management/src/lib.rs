//! case-management —— 功能用例管理
//!
//! 「用例管理」的 Rust 实现:功能用例 CRUD + **自定义字段(jsonb)** +
//! **Excel 导出**。校验(项目/名称非空)是零 IO 纯函数;持久化(PG)、HTTP、xlsx 编码在 adapters。
//! 自定义字段以 `{字段:值}` 字符串映射存 jsonb,适配各团队的模板/字段差异。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
