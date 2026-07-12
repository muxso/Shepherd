//! Acceptance verification context: a Verification checks each acceptance
//! criterion (CriterionStatus), attaches coverage evidence (CoverageLink), and
//! produces CriterionReport/CompletenessReport plus a Gap list. All criteria
//! satisfied is the precondition for a requirement to reach DELIVERED.
//! domain/application/ports do no IO; pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
