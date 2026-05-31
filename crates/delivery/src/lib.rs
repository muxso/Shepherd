//! delivery —— Shepherd 交付执行(任务 → AI 执行者)
//!
//! 把 `task` 上下文派发出来的任务交给 AI 执行者(**Claude Code / Codex**)运行,并把一次次
//! "交付尝试"记录为 `DeliveryAttempt` 聚合(谁、用哪个执行者、到哪一步、产出什么)。
//!
//! 核心是出站端口 **`AgentExecutor`**,复用 api-test 的 `DispatchOutcome` 形态:
//! - `Accepted { run_id }`:执行者**异步**接单(长跑 agent)→ 尝试置 Running,稍后回调收尾;
//! - `Completed { deliverable }`:执行者**同步**跑完 → 尝试直接置 Delivered,带上交付物(diff/PR)。
//!
//! 端口背后两种适配器并存(组装根按环境/任务路由,领域不感知):
//! - `exec-local`:本地 spawn `claude`/`codex` headless 子进程(同步);
//! - `exec-http`:POST 给 Agent SDK/API 服务(异步)。
//!
//! 通过 `decomposition_id` + `task_id` 引用任务,不与 task crate 产生类型耦合。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
