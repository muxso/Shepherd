//! 端口层:协议插件抽象。每种协议一个插件,只管「执行协议、产出原始结果」;断言判定在 domain。

use async_trait::async_trait;

use crate::domain::{ProbeRequest, RawProbe};

/// 协议插件。`protocol()` 返回协议名(注册键);`run()` 执行一次探测,返回原始结果(不含断言)。
#[async_trait]
pub trait ProtocolPlugin: Send + Sync {
    fn protocol(&self) -> &'static str;
    async fn run(&self, req: &ProbeRequest) -> RawProbe;
}
