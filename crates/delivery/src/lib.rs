//! 交付上下文:任务派发到执行者并跟踪交付。核心是 DeliveryAttempt 状态机与 Deliverable 交付物,
//! AgentExecutor 抽象执行者(stub 桩/local 本地/exec-http 远程/exec-queue 机群队列),
//! WorkQueue + FleetRegistry 支撑无公网 agent 的 pull 式领取与心跳注册,ExecutionEvent 记录执行过程。
//! domain/application/ports 不做 IO;pg/http/redis 等适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
