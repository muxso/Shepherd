//! Design proposal context: design draft and human approval gate before requirement breakdown.
//! Proposal state machine: DRAFTING → PENDING_REVIEW → APPROVED / CHANGES_REQUESTED (revision
//! loop); APPROVED is the only terminal state. The DesignDrafter port generates drafts; on
//! approval, BreakdownTrigger kicks off task breakdown.
//! domain/application/ports do no IO; pg/http/agent adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
