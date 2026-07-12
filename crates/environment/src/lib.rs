//! API test environment context: per-project Environment (base_url + shared request headers +
//! variable table), used by case/scenario/batch execution to resolve `${var}` and build request
//! URLs.
//! domain/application/ports do no IO; pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
