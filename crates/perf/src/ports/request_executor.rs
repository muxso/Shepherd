//! 单次请求执行端口:把"如何发一次请求"与"如何并发施压"解耦。
//!
//! 实现者预先持有被测目标(URL/方法/断言等),`execute` 跑一次并返回是否成功;
//! 延迟由引擎用墙钟测量,故端口只回 `bool`,保持最小、易 fake。

use async_trait::async_trait;

#[async_trait]
pub trait RequestExecutor: Send + Sync {
    /// 执行一次请求,返回是否成功(成功的判定由实现决定:HTTP 2xx 或断言全过)。
    async fn execute(&self) -> bool;
}
