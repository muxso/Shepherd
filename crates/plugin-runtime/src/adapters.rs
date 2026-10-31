use crate::{domain::*, ports::*};
use async_trait::async_trait;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct CancellationToken(watch::Sender<bool>);
impl Default for CancellationToken {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }
}
#[async_trait]
impl Cancellation for CancellationToken {
    fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }
    async fn cancelled(&self) {
        let mut receiver = self.0.subscribe();
        loop {
            if *receiver.borrow_and_update() {
                return;
            }
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct PluginRegistry {
    // Store the validated descriptor once; plugins cannot mutate their contract after registration.
    plugins: BTreeMap<String, (PluginDescriptor, Arc<dyn EnvironmentPlugin>)>,
}
impl PluginRegistry {
    pub fn register(&mut self, plugin: Arc<dyn EnvironmentPlugin>) -> Result<(), PluginError> {
        let descriptor = plugin.descriptor();
        descriptor.validate()?;
        if self.plugins.contains_key(&descriptor.id) {
            return Err(PluginError::InvalidContract("duplicate plugin id".into()));
        }
        self.plugins.insert(descriptor.id.clone(), (descriptor, plugin));
        Ok(())
    }
    pub fn descriptors(&self) -> Vec<PluginDescriptor> {
        self.plugins.values().map(|(descriptor, _)| descriptor.clone()).collect()
    }
    pub async fn execute(
        &self,
        invocation: &Invocation,
        context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        context.scope.validate()?;
        context.limits.validate()?;
        if context.cancellation.is_cancelled() {
            return Err(PluginError::Cancelled);
        }
        let (descriptor, plugin) = self
            .plugins
            .get(&invocation.plugin_id)
            .ok_or_else(|| PluginError::Unavailable(invocation.plugin_id.clone()))?;
        if !descriptor.operations.contains(&invocation.operation) {
            return Err(PluginError::UnsupportedOperation(invocation.operation.clone()));
        }
        descriptor.config_schema.validate(&invocation.config)?;
        let output = tokio::select! {
            biased;
            _ = context.cancellation.cancelled() => return Err(PluginError::Cancelled),
            result = tokio::time::timeout(Duration::from_millis(context.limits.timeout_ms), plugin.execute(invocation, context)) => {
                result.map_err(|_| PluginError::Timeout)??
            }
        };
        let bytes = serde_json::to_vec(&output)
            .map_err(|_| PluginError::Execution("invalid plugin output".into()))?;
        if bytes.len() > context.limits.max_output_bytes {
            return Err(PluginError::OutputLimit);
        }
        Ok(output)
    }
}
