//! 应用层:交付编排。依赖 `ports` trait(仓储 + 执行者),不碰具体 IO/进程。
pub mod delivery_service;

pub use delivery_service::{DeliveryCmdError, DeliveryService};
