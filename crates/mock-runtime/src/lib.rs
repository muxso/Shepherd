//! Mock 运行时:接收 MockRequest,按 MatchRule(路径/方法/请求头/请求体匹配 + 附加条件)挑选启用的
//! MockRule 并渲染响应(template feature 启用模板函数)。
//! 规则来自 MockRuleSource 端口(pg 适配器读接口定义里的 ApiMock);http 适配器暴露 mock 入口路由。

pub mod adapters;
pub mod domain;
pub mod ports;
