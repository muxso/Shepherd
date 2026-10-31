use crate::domain::*;
use async_trait::async_trait;
use std::{fmt, sync::Arc};

/// Deliberate opt-in access to plaintext. Debug output never reveals credentials.
pub struct SecretValue(String);
impl SecretValue {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

#[async_trait]
pub trait SecretResolver: Send + Sync {
    async fn resolve(
        &self,
        scope: &ExecutionScope,
        reference: &SecretRef,
    ) -> Result<SecretValue, PluginError>;
}

#[async_trait]
pub trait Cancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
    async fn cancelled(&self);
}

#[derive(Clone)]
pub struct ExecutionContext {
    pub scope: ExecutionScope,
    pub limits: ExecutionLimits,
    pub secrets: Arc<dyn SecretResolver>,
    pub cancellation: Arc<dyn Cancellation>,
}

#[async_trait]
pub trait EnvironmentPlugin: Send + Sync {
    fn descriptor(&self) -> PluginDescriptor;
    /// Implementations must not detach work that outlives this future. Cancellation
    /// and deadlines drop the future; resource guards must release owned resources.
    async fn execute(
        &self,
        invocation: &Invocation,
        context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError>;
}
