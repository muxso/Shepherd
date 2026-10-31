use crate::redaction::Redactor;
use crate::{domain::*, ports::*};
use async_trait::async_trait;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::{watch, Semaphore};

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

#[derive(Clone)]
pub struct PluginRegistry {
    // Store the validated descriptor once; plugins cannot mutate their contract after registration.
    plugins: BTreeMap<String, (PluginDescriptor, Arc<dyn EnvironmentPlugin>)>,
    permits: Arc<Semaphore>,
}
impl Default for PluginRegistry {
    fn default() -> Self {
        Self { plugins: BTreeMap::new(), permits: Arc::new(Semaphore::new(16)) }
    }
}
impl PluginRegistry {
    pub fn with_concurrency(maximum: usize) -> Result<Self, PluginError> {
        if !(1..=1024).contains(&maximum) {
            return Err(PluginError::InvalidConfig("invalid execution concurrency".into()));
        }
        Ok(Self { permits: Arc::new(Semaphore::new(maximum)), ..Self::default() })
    }
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
    pub fn validate_config(&self, id: &str, config: &PluginConfig) -> Result<(), PluginError> {
        let (descriptor, plugin) =
            self.plugins.get(id).ok_or_else(|| PluginError::Unavailable(id.into()))?;
        descriptor.config_schema.validate(config)?;
        plugin.validate_config(config)
    }
    /// The host must authorize configuration reads before calling this pure inspection path.
    pub fn inspect_config(
        &self,
        id: &str,
        config: &PluginConfig,
    ) -> Result<serde_json::Value, PluginError> {
        let (descriptor, plugin) =
            self.plugins.get(id).ok_or_else(|| PluginError::Unavailable(id.into()))?;
        descriptor.config_schema.validate(config)?;
        let metadata = plugin.inspect_config(config)?;
        if serde_json::to_vec(&metadata)
            .map_err(|_| PluginError::Execution("invalid configuration metadata".into()))?
            .len()
            > ExecutionLimits::default().max_output_bytes
        {
            return Err(PluginError::OutputLimit);
        }
        Ok(metadata)
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
        plugin.validate_config(&invocation.config)?;
        tokio::select! {
            biased;
            _ = context.cancellation.cancelled() => Err(PluginError::Cancelled),
            result = tokio::time::timeout(Duration::from_millis(context.limits.timeout_ms), self.authorized_execute(plugin, invocation, context)) => {
                result.map_err(|_| PluginError::Timeout)?
            }
        }
    }
    async fn authorized_execute(
        &self,
        plugin: &Arc<dyn EnvironmentPlugin>,
        invocation: &Invocation,
        context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        context
            .authorization
            .authorize(&context.scope, &invocation.plugin_id, &invocation.operation)
            .await?;
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| PluginError::Unavailable("execution limiter".into()))?;
        let mut values = BTreeMap::new();
        for reference in invocation.config.secrets.values() {
            if !values.contains_key(&reference.0) {
                values.insert(
                    reference.0.clone(),
                    context.secrets.resolve(&context.scope, reference).await?,
                );
            }
        }
        let redactor = Redactor::new(values.values().map(SecretValue::expose))?;
        let secrets = Arc::new(PrefetchedSecrets { scope: context.scope.clone(), values });
        let context = ExecutionContext { secrets, ..context.clone() };
        let output =
            plugin.execute(invocation, &context).await.map_err(|error| redactor.error(error))?;
        let check_size = |output: &PluginOutput| -> Result<(), PluginError> {
            if serde_json::to_vec(output)
                .map_err(|_| PluginError::Execution("invalid plugin output".into()))?
                .len()
                > context.limits.max_output_bytes
            {
                return Err(PluginError::OutputLimit);
            }
            Ok(())
        };
        check_size(&output)?;
        let output = redactor.output(output);
        check_size(&output)?;
        Ok(output)
    }
}

struct PrefetchedSecrets {
    scope: ExecutionScope,
    values: BTreeMap<String, SecretValue>,
}
#[async_trait]
impl SecretResolver for PrefetchedSecrets {
    async fn resolve(
        &self,
        scope: &ExecutionScope,
        reference: &SecretRef,
    ) -> Result<SecretValue, PluginError> {
        if scope != &self.scope {
            return Err(PluginError::PermissionDenied);
        }
        self.values
            .get(&reference.0)
            .map(|v| SecretValue::new(v.expose().to_string()))
            .ok_or(PluginError::PermissionDenied)
    }
}
