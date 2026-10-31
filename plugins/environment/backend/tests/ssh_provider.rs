#![cfg(feature = "protocols")]
#[path = "../../../../crates/probe/tests/support/ssh.rs"]
mod fixture;
use plugin_runtime::*;
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};
struct Policy;
#[async_trait::async_trait]
impl ExecutionAuthorizer for Policy {
    async fn authorize(&self, _: &ExecutionScope, _: &str, _: &str) -> Result<(), PluginError> {
        Ok(())
    }
}
struct Secrets(BTreeMap<String, String>);
#[async_trait::async_trait]
impl SecretResolver for Secrets {
    async fn resolve(
        &self,
        scope: &ExecutionScope,
        reference: &SecretRef,
    ) -> Result<SecretValue, PluginError> {
        if scope.project_id != "project" || scope.environment_id != "environment" {
            return Err(PluginError::PermissionDenied);
        }
        self.0
            .get(&reference.0)
            .map(|value| SecretValue::new(value.clone()))
            .ok_or(PluginError::PermissionDenied)
    }
}
#[tokio::test]
#[ignore = "loopback SSH fixture requires ssh-keygen"]
async fn ssh_profile_authentication_reports_redaction_and_validation() {
    let fixture = fixture::Fixture::start().await;
    let registry = environment_plugins::providers::registry().expect("providers");
    let mut invocation = Invocation {
        plugin_id: "ssh".into(),
        operation: "execute".into(),
        config: PluginConfig {
            schema_version: 1,
            values: [
                ("host".into(), json!(fixture.target.host)),
                ("port".into(), json!(fixture.target.port)),
                ("username".into(), json!(fixture.target.username)),
                ("host_key_sha256".into(), json!(fixture.target.host_key_sha256)),
                ("authentication".into(), json!("password")),
            ]
            .into(),
            secrets: [("password".into(), SecretRef("password".into()))].into(),
        },
        input: json!({"command":"echo fixture password"}),
    };
    let context = ExecutionContext {
        scope: ExecutionScope {
            project_id: "project".into(),
            environment_id: "environment".into(),
            actor_id: "owner".into(),
        },
        limits: ExecutionLimits { timeout_ms: 5000, max_output_bytes: 65_536 },
        secrets: Arc::new(Secrets(
            [
                ("password".into(), "fixture password".into()),
                ("key".into(), fixture.encrypted_key.clone()),
                ("phrase".into(), "fixture passphrase".into()),
            ]
            .into(),
        )),
        authorization: Arc::new(Policy),
        cancellation: Arc::new(CancellationToken::default()),
    };
    let result = registry.execute(&invocation, &context).await.expect("execute");
    assert!(result.passed());
    assert!(!serde_json::to_string(&result).expect("JSON").contains("fixture password"));
    invocation.input = json!({"command":"streams"});
    let result = registry.execute(&invocation, &context).await.expect("report");
    assert_eq!(result.status, OutputStatus::Failure);
    assert_eq!(result.data["termination"]["code"], 7);
    assert_eq!(result.data["stdout"]["value"], "out with spaces\n");
    invocation.config.values.insert("authentication".into(), json!("private_key"));
    invocation.config.secrets = [
        ("private_key".into(), SecretRef("key".into())),
        ("passphrase".into(), SecretRef("phrase".into())),
    ]
    .into();
    invocation.operation = "test".into();
    invocation.input = json!(null);
    let result = registry.execute(&invocation, &context).await.expect("encrypted key profile");
    assert!(result.passed());
    assert_eq!(result.data["operation"], "connected");
    invocation.config.values.insert("host_key_sha256".into(), json!("invalid"));
    assert!(matches!(
        registry.execute(&invocation, &context).await,
        Err(PluginError::InvalidConfig(_))
    ));
    fixture.idle().await;
}
