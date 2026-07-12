//! Functional case context: FunctionalCase aggregate (CaseStep steps + expected results)
//! and its requirement-coverage links (CaseRequirement), the evidence source for
//! acceptance verification.
//! domain/application/ports do no IO; pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
