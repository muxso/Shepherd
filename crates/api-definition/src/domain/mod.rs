//! 领域层:接口定义 / 接口用例 / Mock 三聚合,零 IO。
pub mod api_case;
pub mod api_definition;
pub mod api_module;
pub mod api_mock;
pub mod api_view;
pub mod error;
pub mod import;

pub use api_case::{ApiCase, NewApiCase};
pub use api_view::{ApiView, NewApiView};
pub use api_definition::{
    ApiDefinition, ApiDefinitionChange, ApiProtocol, ApiStatus, NewApiDefinition,
};
pub use api_module::{ApiModule, NewApiModule};
pub use api_mock::{ApiMock, NewApiMock};
pub use error::ApiDefinitionError;
pub use import::{parse_openapi, ImportedApi};
