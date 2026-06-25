//! ms-case —— 阶段 3,用例管理(评审)
//!
//! 本模块的明星是**用例评审状态机**:把 Java 版 `getFunctionalCaseStatus` 里那段
//! 嵌套 if + AtomicInteger 计数的会签/或签聚合逻辑,提炼成零 IO 的纯函数,
//! 用穷举式用例钉死每条分支——这正是 TDD 最该发力、Java 版最难测的地方。
//!
//! 规则:
//! - 单用例评审状态:UnReviewed / UnderReviewed / Pass / UnPass / ReReviewed
//! - 通过规则 PassRule:Single(或签)/ Multiple(会签)
//! - 每个评审人取「最新的非『建议(UnderReviewed)』结论」;SYSTEM 用户不计票
//! - 提交 UnPass 必须带评论内容

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
