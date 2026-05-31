//! ms-kernel —— 阶段 0 共享内核
//!
//! 对应 Java 版 `framework/sdk` 中的纯工具(分页、权限常量、通用错误)。
//! 这一层**零外部依赖、零 IO**,所有逻辑都能用毫秒级 `#[test]` 覆盖。
//! TDD 起点就放在这里:先把"共享词汇"用测试钉死,上层才有稳定地基。

pub mod page;
pub mod permission;
