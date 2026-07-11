//! 跨上下文共享原语:分页(PageRequest/Page,页大小上限 500)与
//! 权限(Permission/PermissionSet,按 resource:action 判定)。
//! 无 IO、无框架依赖,任何上下文可安全引用。

pub mod page;
pub mod permission;
