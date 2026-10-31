#![cfg(feature = "protocols")]
use api_runner::Processor;
use async_trait::async_trait;
use environment_plugins::{domain::*, ports::ProfileStore, preparation::Preparation};
use plugin_runtime::*;
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
#[derive(Default)]
struct Store {
    profiles: BTreeMap<String, Profile>,
    bindings: Vec<Binding>,
    reads: AtomicUsize,
}
#[async_trait]
impl ProfileStore for Store {
    async fn get(&self, project: &str, id: &str) -> Result<Profile, StoreError> {
        self.profiles
            .get(id)
            .filter(|p| p.project_id == project)
            .cloned()
            .ok_or(StoreError::NotFound)
    }
    async fn bindings(&self, project: &str, environment: &str) -> Result<Vec<Binding>, StoreError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .bindings
            .iter()
            .filter(|b| b.project_id == project && b.environment_id == environment)
            .cloned()
            .collect())
    }
    async fn create(&self, _: &str, _: NewProfile) -> Result<Profile, StoreError> {
        Err(StoreError::Unavailable)
    }
    async fn list(&self, _: &str) -> Result<Vec<Profile>, StoreError> {
        Err(StoreError::Unavailable)
    }
    async fn update(&self, _: &str, _: &str, _: i64, _: NewProfile) -> Result<Profile, StoreError> {
        Err(StoreError::Unavailable)
    }
    async fn delete(&self, _: &str, _: &str, _: i64) -> Result<(), StoreError> {
        Err(StoreError::Unavailable)
    }
    async fn bind(&self, _: Binding) -> Result<(), StoreError> {
        Err(StoreError::Unavailable)
    }
    async fn unbind(&self, _: &str, _: &str, _: BindingSlot, _: &str) -> Result<(), StoreError> {
        Err(StoreError::Unavailable)
    }
}
struct Policy(bool);
#[async_trait]
impl ExecutionAuthorizer for Policy {
    async fn authorize(&self, scope: &ExecutionScope, _: &str, _: &str) -> Result<(), PluginError> {
        if self.0 && scope.project_id == "p" {
            Ok(())
        } else {
            Err(PluginError::PermissionDenied)
        }
    }
}
struct DeniedSecrets;
#[async_trait]
impl SecretResolver for DeniedSecrets {
    async fn resolve(&self, _: &ExecutionScope, _: &SecretRef) -> Result<SecretValue, PluginError> {
        Err(PluginError::PermissionDenied)
    }
}
fn context(token: &CancellationToken) -> ExecutionContext {
    ExecutionContext {
        scope: ExecutionScope {
            project_id: "p".into(),
            environment_id: "e".into(),
            actor_id: "owner".into(),
        },
        limits: ExecutionLimits { timeout_ms: 1000, max_output_bytes: 65_536 },
        authorization: Arc::new(Policy(true)),
        secrets: Arc::new(DeniedSecrets),
        cancellation: Arc::new(token.clone()),
    }
}
fn profile(id: &str, plugin: &str, values: serde_json::Value) -> Profile {
    Profile {
        id: id.into(),
        project_id: "p".into(),
        plugin_id: plugin.into(),
        name: id.into(),
        enabled: true,
        revision: 1,
        config: PluginConfig {
            schema_version: 1,
            values: serde_json::from_value(values).expect("values"),
            secrets: BTreeMap::new(),
        },
    }
}
fn binding(id: &str, slot: BindingSlot, priority: i32) -> Binding {
    Binding {
        project_id: "p".into(),
        environment_id: "e".into(),
        profile_id: id.into(),
        slot,
        priority,
        enabled: true,
    }
}
struct Data(Arc<Mutex<Vec<String>>>);
#[async_trait]
impl EnvironmentPlugin for Data {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "fixture".into(),
            name: "Fixture".into(),
            section: "database".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            capabilities: vec![Capability::Execute],
            operations: vec!["execute".into()],
            config_schema: ConfigSchema {
                version: 1,
                fields: vec![ConfigField {
                    name: "credential".into(),
                    label: "Credential".into(),
                    required: false,
                    kind: FieldType::Secret,
                }],
            },
        }
    }
    async fn execute(
        &self,
        inv: &Invocation,
        _: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        let value = inv.input["value"].as_str().unwrap_or("none").to_string();
        self.0.lock().expect("trace").push(value.clone());
        let mut output = PluginOutput::success(json!({"value":value}));
        output.variables.insert("db".into(), value);
        Ok(output)
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preparation_orders_environment_before_case_and_preflights_every_credential() {
    let trace = Arc::new(Mutex::new(Vec::new()));
    let mut registry = environment_plugins::providers::registry().expect("registry");
    registry.register(Arc::new(Data(trace.clone()))).expect("fixture");
    let first = profile(
        "first",
        "setup",
        json!({"steps":[{"type":"variables","values":{"x":"environment"}},{"type":"invoke","profileId":"data","input":{"value":"{{x}}"},"resultVariable":"report"}]}),
    );
    let second = profile("second", "javascript", json!({"code":"vars.x += ':second';"}));
    let data = profile("data", "fixture", json!({}));
    let mut store = Store {
        profiles: [("first".into(), first), ("second".into(), second), ("data".into(), data)]
            .into(),
        bindings: vec![
            binding("second", BindingSlot::Pre, 10),
            binding("first", BindingSlot::Pre, -10),
            binding("data", BindingSlot::Connection, 0),
        ],
        ..Store::default()
    };
    let token = CancellationToken::default();
    let context = context(&token);
    let processors = [
        Processor::Script { lang: "JS".into(), code: "vars.x += ':case';".into() },
        Processor::Plugin { profile_id: "second".into() },
    ];
    let result = Preparation { store: &store, registry: &registry, context: &context }
        .run([("x".into(), "seed".into())].into(), &processors)
        .await
        .expect("prepare");
    assert_eq!(result.variables["x"], "environment:second:case:second");
    assert_eq!(result.variables["db"], "environment");
    assert_eq!(trace.lock().expect("trace").as_slice(), ["environment"]);
    assert_eq!(result.steps.len(), 5);
    assert!(result.steps[0].source.starts_with("environment:first"));
    assert!(result.steps[3].source.starts_with("case:1"));
    trace.lock().expect("trace").clear();
    let mut denied = profile("denied", "fixture", json!({}));
    denied.config.secrets.insert("credential".into(), SecretRef("foreign-credential".into()));
    store.profiles.insert("denied".into(), denied);
    store.bindings.push(binding("denied", BindingSlot::Connection, 0));
    let processor = Processor::Sql {
        name: "denied".into(),
        datasource: "denied".into(),
        sql: "SELECT 1".into(),
    };
    let result = Preparation { store: &store, registry: &registry, context: &context }
        .run(BTreeMap::new(), &[processor])
        .await;
    assert!(matches!(result, Err(PluginError::PermissionDenied)));
    assert!(
        trace.lock().expect("trace").is_empty(),
        "earlier data-source steps must not execute when a later credential is denied"
    );
}
#[tokio::test]
async fn preparation_obeys_scope_total_deadline_cancellation_and_output_limit() {
    let store = Store::default();
    let registry = environment_plugins::providers::registry().expect("registry");
    let token = CancellationToken::default();
    let mut context = context(&token);
    context.authorization = Arc::new(Policy(false));
    assert!(matches!(
        Preparation { store: &store, registry: &registry, context: &context }
            .run(BTreeMap::new(), &[])
            .await,
        Err(PluginError::PermissionDenied)
    ));
    assert_eq!(store.reads.load(Ordering::SeqCst), 0);
    context.authorization = Arc::new(Policy(true));
    context.limits.timeout_ms = 10;
    assert!(matches!(
        Preparation { store: &store, registry: &registry, context: &context }
            .run(BTreeMap::new(), &[Processor::Wait { ms: 1000 }])
            .await,
        Err(PluginError::Timeout)
    ));
    context.limits.timeout_ms = 1000;
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        token.cancel();
    };
    let processors = [Processor::Wait { ms: 1000 }];
    let prepare = Preparation { store: &store, registry: &registry, context: &context };
    let (result, ()) = tokio::join!(prepare.run(BTreeMap::new(), &processors), cancel);
    assert!(matches!(result, Err(PluginError::Cancelled)));
    let token = CancellationToken::default();
    context.cancellation = Arc::new(token);
    context.limits.max_output_bytes = 10;
    assert!(matches!(
        Preparation { store: &store, registry: &registry, context: &context }
            .run(BTreeMap::new(), &[])
            .await,
        Err(PluginError::OutputLimit)
    ));
}
