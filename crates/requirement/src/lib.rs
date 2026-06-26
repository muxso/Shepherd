//! 核心规则:同项目内标题唯一(忽略软删除,删后可重建同名);修订追加不可变版本快照;
//! baseline 显式指向某个已存在版本,修订不自动移动基线。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
