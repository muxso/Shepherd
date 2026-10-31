#![cfg(feature = "ssh")]
#[path = "support/ssh.rs"]
mod fixture;
use fixture::Fixture;
use probe::{
    domain::ssh::{SshAction, SshAuthentication, SshCommand, SshOutput, SshReport, SshTermination},
    plugins::{
        ssh_command::{SshBudget, SshError},
        SshPlugin,
    },
};
fn budget() -> SshBudget {
    SshBudget { timeout_ms: 5000, max_bytes: 65_536 }
}
#[tokio::test]
#[ignore = "loopback SSH fixture requires ssh-keygen"]
async fn pinned_ssh_authentication_streams_status_limits_and_cleanup() {
    let fixture = Fixture::start().await;
    let driver = SshPlugin::new();
    let connected = driver
        .execute_command(
            &fixture.target,
            SshAuthentication::Password("fixture password"),
            SshAction::Test,
            budget(),
        )
        .await
        .expect("password auth");
    assert!(connected.success());
    fixture.idle().await;
    let mut wrong = fixture.target.clone();
    wrong.host_key_sha256 = fixture.wrong_fingerprint.clone();
    let attempts = fixture.attempts();
    assert!(matches!(
        driver
            .execute_command(
                &wrong,
                SshAuthentication::Password("fixture password"),
                SshAction::Test,
                budget()
            )
            .await,
        Err(SshError::HostKeyMismatch)
    ));
    assert_eq!(fixture.attempts(), attempts, "host key must be verified before authentication");
    fixture.idle().await;
    assert!(matches!(
        driver
            .execute_command(
                &fixture.target,
                SshAuthentication::Password("wrong password"),
                SshAction::Test,
                budget()
            )
            .await,
        Err(SshError::Authentication)
    ));
    let key = driver
        .execute_command(
            &fixture.target,
            SshAuthentication::PrivateKey { key: &fixture.private_key, passphrase: None },
            SshAction::Test,
            budget(),
        )
        .await
        .expect("key auth");
    assert!(key.success());
    let encrypted = driver
        .execute_command(
            &fixture.target,
            SshAuthentication::PrivateKey {
                key: &fixture.encrypted_key,
                passphrase: Some("fixture passphrase"),
            },
            SshAction::Test,
            budget(),
        )
        .await
        .expect("encrypted key auth");
    assert!(encrypted.success());
    let command = SshCommand { command: "streams".into() };
    let report = driver
        .execute_command(
            &fixture.target,
            SshAuthentication::Password("fixture password"),
            SshAction::Execute(&command),
            budget(),
        )
        .await
        .expect("streams");
    assert!(!report.success());
    match report {
        SshReport::Executed {
            stdout: SshOutput::Utf8(stdout),
            stderr: SshOutput::Utf8(stderr),
            termination: SshTermination::Exit { code: 7 },
            ..
        } => {
            assert_eq!(stdout, "out with spaces\n");
            assert_eq!(stderr, "error output\n");
        }
        _ => panic!("incorrect stream or exit report"),
    }
    for command in ["missing-status", "signal"] {
        let command = SshCommand { command: command.into() };
        let report = driver
            .execute_command(
                &fixture.target,
                SshAuthentication::Password("fixture password"),
                SshAction::Execute(&command),
                budget(),
            )
            .await
            .expect("termination report");
        assert!(!report.success());
    }
    let command = SshCommand { command: "binary".into() };
    let report = driver
        .execute_command(
            &fixture.target,
            SshAuthentication::Password("fixture password"),
            SshAction::Execute(&command),
            budget(),
        )
        .await
        .expect("binary report");
    assert!(
        matches!(report,SshReport::Executed {stdout:SshOutput::Hex(value),..} if value=="ff00")
    );
    let command = SshCommand { command: "large".into() };
    assert!(matches!(
        driver
            .execute_command(
                &fixture.target,
                SshAuthentication::Password("fixture password"),
                SshAction::Execute(&command),
                SshBudget { max_bytes: 128, ..budget() }
            )
            .await,
        Err(SshError::OutputLimit)
    ));
    fixture.idle().await;
    let command = SshCommand { command: "block".into() };
    assert!(matches!(
        driver
            .execute_command(
                &fixture.target,
                SshAuthentication::Password("fixture password"),
                SshAction::Execute(&command),
                SshBudget { timeout_ms: 50, ..budget() }
            )
            .await,
        Err(SshError::Timeout)
    ));
    let raw = probe::ProbeRequest {
        protocol: "ssh".into(),
        target: format!("{}:{}", fixture.target.host, fixture.target.port),
        payload: Some("echo legacy".into()),
        metadata: [
            ("user".into(), "fixture".into()),
            ("password".into(), "fixture password".into()),
            ("host_key_sha256".into(), fixture.target.host_key_sha256.clone()),
        ]
        .into(),
        assertions: vec![],
    };
    let result = probe::ProtocolPlugin::run(&driver, &raw).await;
    assert!(result.transport_ok);
    assert_eq!(result.status, Some(0));
    assert_eq!(result.output.as_deref(), Some("legacy"));
    fixture.idle().await;
}
