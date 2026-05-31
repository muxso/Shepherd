//! 应用层:编排用例。
pub mod delivery_feedback;

pub use delivery_feedback::{
    DeliveryFeedbackOrchestrator, DeliveryProgress, FeedbackOutcome, VerificationSync,
};
