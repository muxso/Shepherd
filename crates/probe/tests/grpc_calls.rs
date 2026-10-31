#![cfg(feature = "grpc-dynamic")]
#[path = "support/grpc.rs"]
mod fixture;
use fixture::Fixture;
use probe::{
    domain::grpc::{GrpcCall, GrpcReport},
    plugins::{
        grpc_dynamic::{
            CompiledSchema, GrpcAction, GrpcBudget, GrpcCredential, GrpcError, GrpcIdentity,
            GrpcTarget,
        },
        GrpcPlugin,
    },
};
use serde_json::json;
use std::{collections::BTreeMap, sync::atomic::Ordering};
#[tokio::test]
async fn dynamic_unary_protobuf_json_tls_credentials_status_and_limits() {
    let schema = CompiledSchema::compile(&Fixture::files()).expect("schema");
    let driver = GrpcPlugin::new();
    for tls in [false, true] {
        let fixture = Fixture::start(tls).await;
        let metadata = BTreeMap::new();
        let mut target = GrpcTarget {
            endpoint: &fixture.endpoint,
            server_name: tls.then_some("localhost"),
            ca_certificate: tls.then_some(fixture.ca.as_str()),
            identity: tls.then_some(GrpcIdentity {
                certificate: &fixture.certificate,
                private_key: &fixture.key,
            }),
            metadata: &metadata,
            credential: Some(GrpcCredential {
                name: "authorization",
                value: "Bearer fixture-token",
            }),
        };
        let budget = GrpcBudget { timeout_ms: 3000, max_bytes: 65_536 };
        let mut call = GrpcCall {
            method: "/fixture.Echo/Call".into(),
            message: json!({"value":"hello 世界","count":"9223372036854775807","time":"2026-10-31T00:00:00Z","data":"AP8="}),
            metadata: BTreeMap::new(),
        };
        let result = driver
            .execute_call(&schema, &target, GrpcAction::Call(&call), budget)
            .await
            .expect("call");
        assert!(result.success(), "{result:?}");
        if let GrpcReport::Called { response, metadata, .. } = result {
            assert_eq!(response, Some(call.message.clone()));
            assert!(metadata.contains_key("x-bytes-bin"));
        } else {
            panic!("call report");
        }
        call.message = json!({"value":"fail"});
        let result = driver
            .execute_call(&schema, &target, GrpcAction::Call(&call), budget)
            .await
            .expect("application status");
        assert!(!result.success());
        assert!(matches!(result,GrpcReport::Called{status,..} if status.code==7));
        target.credential =
            Some(GrpcCredential { name: "authorization", value: "wrong credential" });
        let unauthenticated = driver
            .execute_call(&schema, &target, GrpcAction::Call(&call), budget)
            .await
            .expect("authentication status");
        assert!(matches!(unauthenticated,GrpcReport::Called{status,..} if status.code==16));
        target.credential =
            Some(GrpcCredential { name: "authorization", value: "Bearer fixture-token" });
        call.metadata.insert("authorization".into(), "override".into());
        assert!(matches!(
            driver.execute_call(&schema, &target, GrpcAction::Call(&call), budget).await,
            Err(GrpcError::InvalidMetadata)
        ));
        call.metadata.clear();
        call.method = "/fixture.Echo/Stream".into();
        assert!(matches!(
            driver.execute_call(&schema, &target, GrpcAction::Call(&call), budget).await,
            Err(GrpcError::Schema(_))
        ));
        call.method = "/fixture.Echo/Call".into();
        let attempts = fixture.calls.load(Ordering::SeqCst);
        call.message = json!({"unknown":true});
        assert!(matches!(
            driver.execute_call(&schema, &target, GrpcAction::Call(&call), budget).await,
            Err(GrpcError::InvalidMessage)
        ));
        assert_eq!(fixture.calls.load(Ordering::SeqCst), attempts);
        call.message = json!({"value":"large"});
        assert!(!driver
            .execute_call(
                &schema,
                &target,
                GrpcAction::Call(&call),
                GrpcBudget { max_bytes: 2048, ..budget }
            )
            .await
            .expect("oversized wire status")
            .success());
        call.message = json!({"value":"block"});
        let result = driver
            .execute_call(
                &schema,
                &target,
                GrpcAction::Call(&call),
                GrpcBudget { timeout_ms: 100, ..budget },
            )
            .await;
        assert!(matches!(result, Err(GrpcError::Timeout)) || result.is_ok_and(|r| !r.success()));
        fixture.idle().await;
        if tls {
            target.server_name = Some("wrong.example");
            assert!(driver.execute_call(&schema, &target, GrpcAction::Test, budget).await.is_err());
            fixture.idle().await;
            target.server_name = Some("localhost");
            target.ca_certificate = None;
            assert!(driver.execute_call(&schema, &target, GrpcAction::Test, budget).await.is_err());
            fixture.idle().await;
            target.ca_certificate = Some(&fixture.ca);
            target.identity = None;
            call.message = json!({"value":"hello"});
            let result =
                driver.execute_call(&schema, &target, GrpcAction::Call(&call), budget).await;
            assert!(result.is_err() || result.is_ok_and(|r| !r.success()));
        }
        fixture.idle().await;
    }
}
