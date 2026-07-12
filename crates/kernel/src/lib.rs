//! Primitives shared across contexts: pagination (PageRequest/Page, page size
//! capped at 500) and permissions (Permission/PermissionSet, checked by
//! resource:action). No IO, no framework deps; safe for any context to depend on.

pub mod page;
pub mod permission;
