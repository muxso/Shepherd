//! 接口测试环境上下文:Environment(base_url + 公共请求头 + 变量表)按项目维护,
//! 供用例/场景/批量执行时解析 `${var}` 与拼接请求地址。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
