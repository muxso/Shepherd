use async_trait::async_trait;
use plugin_runtime::*;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

fn schema() -> ConfigSchema {
    ConfigSchema {
        version: 1,
        fields: vec![
            ConfigField {
                name: "host".into(),
                label: "Host".into(),
                required: true,
                kind: FieldType::String { max_length: 100 },
            },
            ConfigField {
                name: "port".into(),
                label: "Port".into(),
                required: true,
                kind: FieldType::Integer { min: 1, max: 65535 },
            },
            ConfigField {
                name: "password".into(),
                label: "Password".into(),
                required: true,
                kind: FieldType::Secret,
            },
        ],
    }
}
fn config() -> PluginConfig {
    PluginConfig {
        schema_version: 1,
        values: [("host".into(), json!("localhost")), ("port".into(), json!(5432))].into(),
        secrets: [("password".into(), SecretRef("project/p/connection-password".into()))].into(),
    }
}
fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "test.database".into(),
        name: "Test database".into(),
        section: "database".into(),
        version: "1.0.0".into(),
        api_version: PLUGIN_API_VERSION,
        capabilities: vec![Capability::Execute, Capability::ConnectionTest],
        operations: vec!["query".into(), "test".into()],
        config_schema: schema(),
    }
}
fn invocation() -> Invocation {
    Invocation {
        plugin_id: "test.database".into(),
        operation: "query".into(),
        config: config(),
        input: json!({"query":"SELECT 1"}),
    }
}
struct NoSecrets;
#[async_trait]
impl SecretResolver for NoSecrets {
    async fn resolve(
        &self,
        _scope: &ExecutionScope,
        _reference: &SecretRef,
    ) -> Result<SecretValue, PluginError> {
        Ok(SecretValue::new("fixture-credential".into()))
    }
}
struct Allow;
#[async_trait]
impl ExecutionAuthorizer for Allow {
    async fn authorize(
        &self,
        _scope: &ExecutionScope,
        _plugin: &str,
        _operation: &str,
    ) -> Result<(), PluginError> {
        Ok(())
    }
}
fn context(token: &CancellationToken) -> ExecutionContext {
    ExecutionContext {
        scope: ExecutionScope {
            project_id: "p".into(),
            environment_id: "e".into(),
            actor_id: "user".into(),
        },
        limits: ExecutionLimits::default(),
        cancellation: Arc::new(token.clone()),
        secrets: Arc::new(NoSecrets),
        authorization: Arc::new(Allow),
    }
}
struct Guard(Arc<AtomicBool>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
struct Fake {
    descriptor: PluginDescriptor,
    calls: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
    delay: bool,
}
impl Fake {
    fn new(delay: bool) -> Self {
        Self {
            descriptor: descriptor(),
            calls: Arc::new(AtomicUsize::new(0)),
            dropped: Arc::new(AtomicBool::new(false)),
            delay,
        }
    }
}
#[async_trait]
impl EnvironmentPlugin for Fake {
    fn descriptor(&self) -> PluginDescriptor {
        self.descriptor.clone()
    }
    async fn execute(
        &self,
        invocation: &Invocation,
        _context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        let _guard = Guard(self.dropped.clone());
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.delay {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
        Ok(PluginOutput::success(invocation.input.clone()))
    }
}

#[test]
fn typed_configuration_rejects_unknown_fields_versions_types_and_plaintext_secrets() {
    schema().validate(&config()).expect("valid profile");
    for patch in [
        json!({"schemaVersion":2}),
        json!({"values":{"host":"localhost","port":"5432"}}),
        json!({"values":{"host":"localhost","port":0}}),
        json!({"values":{"host":"localhost","port":5432,"surprise":true}}),
        json!({"values":{"host":"localhost","port":5432,"password":"DO-NOT-LOG"}}),
        json!({"secrets":{}}),
        json!({"secrets":{"password":""}}),
        json!({"secrets":{"password":"secret/pw","host":"secret/host"}}),
    ] {
        let mut value = serde_json::to_value(config()).expect("serialize");
        for (key, patch) in patch.as_object().expect("object") {
            value[key] = patch.clone();
        }
        let config: PluginConfig = serde_json::from_value(value).expect("decode");
        let error = schema().validate(&config).expect_err("invalid configuration");
        assert!(!error.to_string().contains("DO-NOT-LOG"));
    }
    assert!(
        serde_json::from_value::<PluginConfig>(json!({"schemaVersion":1,"unknown":42})).is_err()
    );
}

#[test]
fn raw_configuration_and_secret_debug_output_are_redacted() {
    let config = PluginConfig {
        values: [("password".into(), json!("DO-NOT-LOG"))].into(),
        ..Default::default()
    };
    assert!(!format!("{config:?}").contains("DO-NOT-LOG"));
    let secret = SecretValue::new("DO-NOT-LOG".into());
    assert!(!format!("{secret:?}").contains("DO-NOT-LOG"));
    assert_eq!(secret.expose(), "DO-NOT-LOG");
}

#[test]
fn registration_rejects_incompatible_or_ambiguous_contracts() {
    let mut registry = PluginRegistry::default();
    registry.register(Arc::new(Fake::new(false))).expect("register");
    assert!(registry.register(Arc::new(Fake::new(false))).is_err());
    for change in ["api", "id", "operation", "fields", "bounds"] {
        let mut fake = Fake::new(false);
        match change {
            "api" => fake.descriptor.api_version += 1,
            "id" => fake.descriptor.id = "Invalid Id".into(),
            "operation" => fake.descriptor.operations.push("query".into()),
            "fields" => fake
                .descriptor
                .config_schema
                .fields
                .push(fake.descriptor.config_schema.fields[0].clone()),
            _ => {
                fake.descriptor.config_schema.fields[1].kind = FieldType::Integer { min: 5, max: 1 }
            }
        }
        assert!(PluginRegistry::default().register(Arc::new(fake)).is_err());
    }
    assert_eq!(registry.descriptors(), vec![descriptor()]);
}

#[tokio::test]
async fn routing_validates_before_any_plugin_io() {
    let fake = Arc::new(Fake::new(false));
    let mut registry = PluginRegistry::default();
    registry.register(fake.clone()).expect("register");
    let mut request = invocation();
    let mut context = context(&CancellationToken::default());
    context.scope.environment_id.clear();
    assert!(matches!(
        registry.execute(&request, &context).await,
        Err(PluginError::PermissionDenied)
    ));
    context.scope.environment_id = "e".into();
    request.operation = "unsupported".into();
    assert!(matches!(
        registry.execute(&request, &context).await,
        Err(PluginError::UnsupportedOperation(_))
    ));
    request = invocation();
    request.config.values.remove("host");
    assert!(matches!(
        registry.execute(&request, &context).await,
        Err(PluginError::InvalidConfig(_))
    ));
    request = invocation();
    request.plugin_id = "not-installed".into();
    assert!(matches!(registry.execute(&request, &context).await, Err(PluginError::Unavailable(_))));
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    let result = registry.execute(&invocation(), &context).await.expect("execute");
    assert!(result.passed());
    assert_eq!(result.data, json!({"query":"SELECT 1"}));
}

#[tokio::test(start_paused = true)]
async fn deadlines_drop_the_operation_and_its_resource_guards() {
    let fake = Arc::new(Fake::new(true));
    let mut registry = PluginRegistry::default();
    registry.register(fake.clone()).expect("register");
    let mut context = context(&CancellationToken::default());
    context.limits.timeout_ms = 10;
    assert!(matches!(registry.execute(&invocation(), &context).await, Err(PluginError::Timeout)));
    assert!(fake.dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn cancellation_before_and_during_execution_is_observed_without_lost_notifications() {
    let fake = Arc::new(Fake::new(true));
    let mut registry = PluginRegistry::default();
    registry.register(fake.clone()).expect("register");
    let token = CancellationToken::default();
    token.cancel();
    assert!(matches!(
        registry.execute(&invocation(), &context(&token)).await,
        Err(PluginError::Cancelled)
    ));
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    let token = CancellationToken::default();
    let context = context(&token);
    let invocation = invocation();
    let (result, _) = tokio::join!(registry.execute(&invocation, &context), async {
        while fake.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        token.cancel();
    });
    assert!(matches!(result, Err(PluginError::Cancelled)));
    assert!(fake.dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn output_limits_cover_the_entire_report_and_failed_assertions_remain_failures() {
    let mut registry = PluginRegistry::default();
    registry.register(Arc::new(Fake::new(false))).expect("register");
    let mut context = context(&CancellationToken::default());
    context.limits.max_output_bytes = 1;
    assert!(matches!(
        registry.execute(&invocation(), &context).await,
        Err(PluginError::OutputLimit)
    ));
    let output = PluginOutput {
        status: OutputStatus::Success,
        data: Value::Null,
        variables: BTreeMap::new(),
        assertions: vec![AssertionResult {
            id: "check-1".into(),
            passed: false,
            message: "expected value missing".into(),
        }],
    };
    assert!(!output.passed());
}
