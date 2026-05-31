//! orchestrator —— Shepherd 跨上下文编排(process manager)
//!
//! 把 **delivery 的交付结果自动回灌 verification**:一次交付尝试进入终态(Delivered/Failed)后,
//! 据其归属的需求版本找到对应验证,把该任务的覆盖链 `satisfied` 同步为"是否交付成功"。
//!
//! 设计:本 crate 不依赖 task/verification/delivery 任何业务 crate,只在自己定义的 gateway 端口
//! (`DecompositionGateway` / `VerificationGateway`)上工作 —— 纯协调、可用 fake 完整单测。
//! 组装根负责:① 把这些 gateway 接到 task / verification 的真实服务;
//! ② 把 delivery 的 `DeliveryObserver` 钩子桥接到本编排器。上下文彼此仍不相互依赖。

pub mod application;
pub mod ports;
