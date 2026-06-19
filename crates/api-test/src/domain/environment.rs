//! 已解析的运行环境(执行器注入用)。
//!
//! 由 `EnvironmentPort` 从某个 environmentId 解析而来,承载执行时要注入到每条请求的
//! base_url + 默认头 + 变量。纯数据(零 IO),便于在 application/ports 间传递与穷举测试。
//! 真正"应用到 RequestSpec"的逻辑落在 adapters/local(那里才认得 api-runner 的 RequestSpec)。

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedEnv {
    /// 相对 URL 的前缀(末尾无斜杠);为空表示不拼接。
    pub base_url: String,
    /// 每次请求并入的默认头(典型:Authorization)。
    pub headers: Vec<(String, String)>,
    /// `${name}` 变量表。
    pub variables: BTreeMap<String, String>,
}

impl ResolvedEnv {
    /// 是否为空环境(无任何注入)。
    pub fn is_empty(&self) -> bool {
        self.base_url.is_empty() && self.headers.is_empty() && self.variables.is_empty()
    }
}
