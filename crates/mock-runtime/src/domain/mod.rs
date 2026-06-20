//! 领域层:Mock 匹配引擎(零 IO 纯函数)。
pub mod mock;

pub use mock::{
    match_request, BodyMatch, MatchRule, MockRequest, MockResponse, MockRule, StringMatch,
};
