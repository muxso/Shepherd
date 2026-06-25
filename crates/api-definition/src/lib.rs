//! api-definition —— 接口定义上下文。
//!
//! 本限界上下文聚合了接口测试的三类核心资产:
//! - **接口定义**(ApiDefinition):协议 + 方法 + 路径 + 状态的元数据,是接口资产的"主数据"。
//! - **接口用例**(ApiCase):挂在某个接口定义下的可执行用例,带断言(JSON 数组)。
//! - **Mock**(ApiMock):对某个接口定义的挡板配置,带匹配规则(JSON)与响应。
//!
//! 校验规则(方法白名单、协议大小写、断言必须是数组、Mock 状态码区间等)全部提炼到
//! domain 层,做成零 IO 的纯函数,用例可逐条单测——Java 版这套逻辑散落在 Service +
//! MyBatis 里,几乎无法单测。

// 导入解析模块的文档含多级列表;放行 pedantic 的延续行缩进 lint(渲染正常)。
#![allow(clippy::doc_lazy_continuation)]

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
