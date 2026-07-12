//! Generic comment context: a Comment attaches to any resource (bug/requirement/case/…)
//! via (target_type, target_id); create/delete/list go through the CommentRepository port.
//! domain/application/ports do no IO; pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
