//! runner —— 中央侧 runner 闭环(对应 `runner-agent` 二进制)
//!
//! 让 Shepherd 能「按环境注册多个 agent → 把自包含用例派给选定环境的 agent 就地执行 → 回传结果」。
//! agent 部署在目标环境内执行,中央只持有注册表 + 派发逻辑,不必把整套系统搬到各环境。
//! 校验(名称/地址非空)纯函数;注册表(PG)、派发(reqwest 调 agent /run)、HTTP 入口在 adapters。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
