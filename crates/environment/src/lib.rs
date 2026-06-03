//! environment —— 环境上下文。
//!
//! 环境(Environment)隶属项目,承载接口/用例/场景执行时的**基础地址 + 默认请求头 + 变量**:
//! - `base_url`:相对 URL 的前缀(执行器拼接);
//! - `headers`:每次请求并入的默认头(典型用途:`Authorization: Bearer …`,解掉受保护接口的 401);
//! - `variables`:`${name}` 占位符的取值表(执行器替换 url/headers/body)。
//!
//! 运行时按 `environmentId` 选择,不与具体用例耦合(同一套用例可在多套环境跑)。
//! 校验规则(名称非空、base_url 形态、表头名非空)提炼到 domain 层做成零 IO 纯函数。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
