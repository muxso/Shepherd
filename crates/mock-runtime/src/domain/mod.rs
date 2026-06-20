//! 领域层:Mock 匹配引擎 + 响应模板(零 IO 纯函数)。
pub mod mock;
#[cfg(feature = "template")]
pub mod template;

pub use mock::{
    match_request, BodyMatch, ExtraConditions, MatchRule, MockRequest, MockResponse, MockRule,
    StringMatch,
};
#[cfg(feature = "template")]
pub use template::{render_body, TemplateError};
