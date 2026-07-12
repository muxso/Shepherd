//! Mock runtime: takes a MockRequest, picks the enabled MockRule via MatchRule (path/method/
//! header/body matching + extra conditions) and renders the response (the template feature
//! enables template functions).
//! Rules come from the MockRuleSource port (the pg adapter reads ApiMock from API definitions);
//! the http adapter exposes the mock entry routes.

pub mod adapters;
pub mod domain;
pub mod ports;
