//! 远程执行器上下文:RunnerAgent 注册表(名称/base_url/支持协议/启停),
//! RemoteRunner/RemoteProbe 端口把接口用例与探测请求转发到远端 runner-agent 执行,
//! ExecutionStore 记录执行结果,DispatchTarget 决定本地或指定 agent 执行。
//! domain/application/ports 不做 IO;pg/http/client(reqwest 远程调用)适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
