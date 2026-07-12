//! Bug context: Bug aggregate + per-project status flow machine (StatusFlowGraph,
//! decides legal transitions) + followers + relations to assets like cases/requirements (BugRelation).
//! domain/application/ports do no IO; pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
