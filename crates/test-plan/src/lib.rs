//! Test-plan context: the Plan aggregate (two types: regular plan / plan group) selects cases
//! and tracks execution results, rolling up CaseCounts statistics and reports by CaseStatus;
//! Schedule produces PlanRun execution records on a timer.
//! domain/application/ports do no IO; the pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
