//! 领域层:接口定义 / 接口用例 / Mock 三聚合,零 IO。
pub mod api_case;
pub mod api_definition;
pub mod api_mock;
pub mod error;

pub use api_case::{ApiCase, NewApiCase};
pub use api_definition::{ApiDefinition, ApiProtocol, ApiStatus, NewApiDefinition};
pub use api_mock::{ApiMock, NewApiMock};
pub use error::ApiDefinitionError;
