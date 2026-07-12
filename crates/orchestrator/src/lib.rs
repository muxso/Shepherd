//! Delivery feedback orchestration: after a delivery completes, a Judge reviews
//! the deliverable against the acceptance criteria (built-in AcceptAllJudge/
//! RuleJudge). Failures go through the Reviser revision loop; passes sync
//! acceptance state via VerificationGateway — the final leg of the
//! requirement → dispatch → verify pipeline.
//! Depends only on ports (TaskGateway/VerificationGateway etc.); no IO itself.

pub mod application;
pub mod judges;
pub mod ports;
