//! Follow context: stores a user's Follow relation to any resource (bug/requirement/case/etc).
//! The FollowStore port carries add/remove/query so every module can reuse the "followers"
//! capability.
//! domain/application/ports do no IO; pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
