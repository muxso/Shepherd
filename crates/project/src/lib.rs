//! 业务规则:同组织内项目名唯一,且唯一性忽略已软删除的项目(可重建同名)。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
