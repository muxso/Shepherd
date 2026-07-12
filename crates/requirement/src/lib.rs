//! Core rules: titles are unique per project (soft-deleted rows excluded, so a
//! deleted title can be recreated); each revision appends an immutable version
//! snapshot; the baseline explicitly points at an existing version and does not
//! move automatically on revision.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
