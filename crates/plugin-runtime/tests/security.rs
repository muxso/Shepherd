use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use plugin_runtime::*;
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

const PASSWORD: &str = "sensitive:p@ss word/密码?=";
struct Policy(bool);
#[async_trait]
impl ExecutionAuthorizer for Policy {
    async fn authorize(
        &self,
        scope: &ExecutionScope,
        _plugin: &str,
        _operation: &str,
    ) -> Result<(), PluginError> {
        if self.0 && scope.project_id == "project" && scope.actor_id == "owner" {
            Ok(())
        } else {
            Err(PluginError::PermissionDenied)
        }
    }
}
#[derive(Default)]
struct Secrets {
    calls: AtomicUsize,
    slow: bool,
}
#[async_trait]
impl SecretResolver for Secrets {
    async fn resolve(
        &self,
        _scope: &ExecutionScope,
        _reference: &SecretRef,
    ) -> Result<SecretValue, PluginError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.slow {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
        Ok(SecretValue::new(PASSWORD.into()))
    }
}
#[derive(Default)]
struct Echo {
    calls: AtomicUsize,
    fail: bool,
    slow: AtomicBool,
}
#[async_trait]
impl EnvironmentPlugin for Echo {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "echo".into(),
            version: "1".into(),
            api_version: 1,
            capabilities: vec![Capability::Execute],
            operations: vec!["execute".into()],
            config_schema: ConfigSchema {
                version: 1,
                fields: vec![ConfigField {
                    name: "password".into(),
                    label: "Password".into(),
                    required: true,
                    kind: FieldType::Secret,
                }],
            },
        }
    }
    async fn execute(
        &self,
        invocation: &Invocation,
        context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.slow.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
        let value =
            context.secrets.resolve(&context.scope, &invocation.config.secrets["password"]).await?;
        assert!(context
            .secrets
            .resolve(&context.scope, &SecretRef("undeclared-secret".into()))
            .await
            .is_err());
        let mut foreign = context.scope.clone();
        foreign.environment_id = "foreign".into();
        assert!(context
            .secrets
            .resolve(&foreign, &invocation.config.secrets["password"])
            .await
            .is_err());
        if self.fail {
            return Err(PluginError::Execution(format!("driver error: {}", value.expose())));
        }
        let mut url = url::Url::parse("https://user@redaction.invalid").expect("url");
        url.set_password(Some(value.expose())).expect("password");
        let mut output = PluginOutput::success(
            json!({"raw":value.expose(),"nested":[{"base64":STANDARD.encode(value.expose()),"url":url.to_string()}],"ordinary":42}),
        );
        output.variables.insert("credential".into(), value.expose().into());
        output.assertions.push(AssertionResult {
            id: "check".into(),
            passed: false,
            message: value.expose().into(),
        });
        Ok(output)
    }
}
fn request() -> Invocation {
    Invocation {
        plugin_id: "echo".into(),
        operation: "execute".into(),
        input: json!(null),
        config: PluginConfig {
            schema_version: 1,
            values: BTreeMap::new(),
            secrets: [("password".into(), SecretRef("secret-reference".into()))].into(),
        },
    }
}
fn context(policy: bool, secrets: Arc<Secrets>, token: &CancellationToken) -> ExecutionContext {
    ExecutionContext {
        scope: ExecutionScope {
            project_id: "project".into(),
            environment_id: "environment".into(),
            actor_id: "owner".into(),
        },
        limits: ExecutionLimits::default(),
        secrets,
        authorization: Arc::new(Policy(policy)),
        cancellation: Arc::new(token.clone()),
    }
}

#[tokio::test]
async fn unauthorized_execution_cannot_read_credentials_or_call_a_plugin() {
    let plugin = Arc::new(Echo::default());
    let secrets = Arc::new(Secrets::default());
    let mut registry = PluginRegistry::default();
    registry.register(plugin.clone()).expect("register");
    let context = context(false, secrets.clone(), &CancellationToken::default());
    assert!(matches!(
        registry.execute(&request(), &context).await,
        Err(PluginError::PermissionDenied)
    ));
    assert_eq!(plugin.calls.load(Ordering::SeqCst), 0);
    assert_eq!(secrets.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn configured_credentials_are_redacted_from_nested_output_variables_assertions_and_errors() {
    let secrets = Arc::new(Secrets::default());
    let context = context(true, secrets.clone(), &CancellationToken::default());
    let mut registry = PluginRegistry::default();
    registry.register(Arc::new(Echo::default())).expect("register");
    let output = registry.execute(&request(), &context).await.expect("run");
    let encoded = serde_json::to_string(&output).expect("serialize");
    assert!(!encoded.contains(PASSWORD));
    assert!(!encoded.contains(&STANDARD.encode(PASSWORD)));
    assert!(encoded.contains("[REDACTED]"));
    assert_eq!(output.data["ordinary"], 42);
    assert_eq!(output.assertions[0].message, "[REDACTED]");
    assert!(!output.passed());
    let mut failing = PluginRegistry::default();
    failing.register(Arc::new(Echo { fail: true, ..Default::default() })).expect("register");
    let error = failing.execute(&request(), &context).await.expect_err("failure");
    assert!(!error.to_string().contains(PASSWORD));
    assert!(error.to_string().contains("[REDACTED]"));
}

#[tokio::test(start_paused = true)]
async fn credential_resolution_is_inside_the_execution_deadline() {
    let plugin = Arc::new(Echo::default());
    let mut registry = PluginRegistry::default();
    registry.register(plugin.clone()).expect("register");
    let mut context = context(
        true,
        Arc::new(Secrets { slow: true, ..Default::default() }),
        &CancellationToken::default(),
    );
    context.limits.timeout_ms = 10;
    assert!(matches!(registry.execute(&request(), &context).await, Err(PluginError::Timeout)));
    assert_eq!(plugin.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn queued_work_obeys_limits_and_cancellation_releases_capacity() {
    let plugin = Arc::new(Echo { slow: AtomicBool::new(true), ..Default::default() });
    let mut registry = PluginRegistry::with_concurrency(1).expect("limiter");
    registry.register(plugin.clone()).expect("register");
    let token = CancellationToken::default();
    let first = context(true, Arc::new(Secrets::default()), &token);
    let mut second = context(true, Arc::new(Secrets::default()), &CancellationToken::default());
    second.limits.timeout_ms = 10;
    let request = request();
    let (a, b) = tokio::join!(registry.execute(&request, &first), async {
        while plugin.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        let result = registry.execute(&request, &second).await;
        token.cancel();
        result
    });
    assert!(matches!(a, Err(PluginError::Cancelled)));
    assert!(matches!(b, Err(PluginError::Timeout)));
    assert_eq!(plugin.calls.load(Ordering::SeqCst), 1);
    plugin.slow.store(false, Ordering::SeqCst);
    registry.execute(&request, &second).await.expect("capacity is released");
}
