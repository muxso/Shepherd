//! 已存储用例的可执行规格(零 IO):从中央用例库解析出的「请求 + 断言」,可直接派给 agent。

use api_runner::{Assertion, RequestSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseSpec {
    pub request: RequestSpec,
    pub assertions: Vec<Assertion>,
}
