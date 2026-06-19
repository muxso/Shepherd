//! 资源池领域模型(零 IO)。
//!
//! 资源池是批量/场景执行的执行节点归属。原仓库只有"读端口"(解析项目默认池 +
//! 可用性),没有创建入口,导致 `api batch-run` 必须先手工 `INSERT ms_resource_pool`
//! 才能跑。这里补上创建所需的最小领域规则:**名称非空**。

use thiserror::Error;

/// 资源池视图(读模型 / 创建返回)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePool {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

/// 资源池创建校验错误。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResourcePoolError {
    #[error("resource pool name must not be empty")]
    EmptyName,
}

/// 已校验的待创建资源池(名称去空白且非空)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewResourcePool {
    pub name: String,
    pub enabled: bool,
}

impl NewResourcePool {
    /// 构造并校验:名称去首尾空白后不得为空。
    pub fn new(name: &str, enabled: bool) -> Result<Self, ResourcePoolError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ResourcePoolError::EmptyName);
        }
        Ok(Self { name: name.to_string(), enabled })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts_name() {
        let p = NewResourcePool::new("  本地池 ", true).expect("ok");
        assert_eq!(p.name, "本地池");
        assert!(p.enabled);
    }

    #[test]
    fn rejects_blank_name() {
        assert_eq!(NewResourcePool::new("   ", true).unwrap_err(), ResourcePoolError::EmptyName);
    }
}
