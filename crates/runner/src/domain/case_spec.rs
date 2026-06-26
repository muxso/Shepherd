use api_runner::{Assertion, RequestSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseSpec {
    pub request: RequestSpec,
    pub assertions: Vec<Assertion>,
}
