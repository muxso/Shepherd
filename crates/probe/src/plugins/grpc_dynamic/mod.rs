//! Dynamic unary gRPC, compiled from in-memory protobuf definitions.
mod schema;
pub use schema::{CompiledSchema, MethodInfo, SchemaError};

mod codec;
mod transport;
use crate::domain::grpc::{GrpcCall, GrpcMetadataValue, GrpcReport, GrpcStatus};
use base64::{engine::general_purpose::STANDARD, Engine};
use prost::Message;
use prost_reflect::DynamicMessage;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use tonic::{
    metadata::{KeyAndValueRef, MetadataKey, MetadataMap, MetadataValue},
    Request,
};

#[derive(Debug, thiserror::Error)]
pub enum GrpcError {
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error("invalid gRPC endpoint or TLS options")]
    InvalidTarget,
    #[error("invalid or conflicting gRPC metadata; use credential references for authentication")]
    InvalidMetadata,
    #[error("message does not match the selected protobuf method")]
    InvalidMessage,
    #[error("invalid gRPC execution budget")]
    InvalidBudget,
    #[error("gRPC operation timed out")]
    Timeout,
    #[error("gRPC request or result exceeds output limit")]
    OutputLimit,
    #[error("invalid gRPC response metadata")]
    InvalidResponse,
    #[error("gRPC connection failed: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("gRPC socket failed: {0}")]
    Io(#[from] std::io::Error),
}
pub struct GrpcIdentity<'a> {
    pub certificate: &'a str,
    pub private_key: &'a str,
}
pub struct GrpcCredential<'a> {
    pub name: &'a str,
    pub value: &'a str,
}
pub struct GrpcTarget<'a> {
    pub endpoint: &'a str,
    pub server_name: Option<&'a str>,
    pub ca_certificate: Option<&'a str>,
    pub identity: Option<GrpcIdentity<'a>>,
    pub metadata: &'a BTreeMap<String, String>,
    pub credential: Option<GrpcCredential<'a>>,
}
#[derive(Clone, Copy)]
pub struct GrpcBudget {
    pub timeout_ms: u64,
    pub max_bytes: usize,
}
pub enum GrpcAction<'a> {
    Test,
    Call(&'a GrpcCall),
}
pub fn validate_endpoint(endpoint: &str, tls_options: bool) -> Result<(), GrpcError> {
    let target = GrpcTarget {
        endpoint,
        server_name: None,
        ca_certificate: None,
        identity: None,
        metadata: &BTreeMap::new(),
        credential: None,
    };
    let uri = self::endpoint(&target)?;
    if tls_options && uri.scheme() != "https" {
        return Err(GrpcError::InvalidTarget);
    }
    Ok(())
}
fn endpoint(target: &GrpcTarget<'_>) -> Result<url::Url, GrpcError> {
    let uri = url::Url::parse(target.endpoint).map_err(|_| GrpcError::InvalidTarget)?;
    if !matches!(uri.scheme(), "http" | "https")
        || uri.host_str().is_none()
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.query().is_some()
        || uri.fragment().is_some()
        || uri.path() != "/"
        || (uri.scheme() == "http"
            && (target.server_name.is_some()
                || target.ca_certificate.is_some()
                || target.identity.is_some()))
    {
        return Err(GrpcError::InvalidTarget);
    }
    Ok(uri)
}
fn key(name: &str) -> Result<MetadataKey<tonic::metadata::Ascii>, GrpcError> {
    MetadataKey::from_bytes(name.as_bytes()).map_err(|_| GrpcError::InvalidMetadata)
}
fn protected(name: &str) -> bool {
    name.starts_with("grpc-")
        || matches!(
            name,
            "host" | "te" | "connection" | "transfer-encoding" | "content-type" | "content-length"
        )
}
pub fn validate_metadata(
    values: &BTreeMap<String, String>,
    credential_name: Option<&str>,
) -> Result<(), GrpcError> {
    if values.len() > 64
        || values.iter().map(|(name, value)| name.len() + value.len()).sum::<usize>() > 65_536
    {
        return Err(GrpcError::InvalidMetadata);
    }
    for (name, value) in values {
        if name != &name.to_ascii_lowercase()
            || protected(name)
            || credential_name == Some(name.as_str())
            || matches!(
                name.as_str(),
                "authorization" | "proxy-authorization" | "cookie" | "x-api-key"
            )
        {
            return Err(GrpcError::InvalidMetadata);
        }
        if name.ends_with("-bin") {
            tonic::metadata::MetadataKey::<tonic::metadata::Binary>::from_bytes(name.as_bytes())
                .map_err(|_| GrpcError::InvalidMetadata)?;
            STANDARD.decode(value).map_err(|_| GrpcError::InvalidMetadata)?;
        } else {
            key(name)?;
            value.parse::<MetadataValue<_>>().map_err(|_| GrpcError::InvalidMetadata)?;
        }
    }
    if let Some(name) = credential_name {
        if name != name.to_ascii_lowercase() || protected(name) {
            return Err(GrpcError::InvalidMetadata);
        }
        if name.ends_with("-bin") {
            tonic::metadata::MetadataKey::<tonic::metadata::Binary>::from_bytes(name.as_bytes())
                .map_err(|_| GrpcError::InvalidMetadata)?;
        } else {
            key(name)?;
        }
    }
    Ok(())
}
fn request_metadata(
    target: &GrpcTarget<'_>,
    call: Option<&GrpcCall>,
) -> Result<MetadataMap, GrpcError> {
    let mut values = target.metadata.clone();
    if let Some(call) = call {
        values.extend(call.metadata.clone());
    }
    validate_metadata(&values, target.credential.as_ref().map(|credential| credential.name))?;
    if let Some(credential) = &target.credential {
        values.insert(credential.name.into(), credential.value.into());
    }
    let mut result = MetadataMap::new();
    for (name, value) in values {
        if name.ends_with("-bin") {
            let name = MetadataKey::<tonic::metadata::Binary>::from_bytes(name.as_bytes())
                .map_err(|_| GrpcError::InvalidMetadata)?;
            let bytes = STANDARD.decode(value).map_err(|_| GrpcError::InvalidMetadata)?;
            let mut header = MetadataValue::from_bytes(&bytes);
            header.set_sensitive(
                target
                    .credential
                    .as_ref()
                    .is_some_and(|credential| credential.name == name.as_str()),
            );
            result.insert_bin(name, header);
        } else {
            let mut header = value
                .parse::<MetadataValue<tonic::metadata::Ascii>>()
                .map_err(|_| GrpcError::InvalidMetadata)?;
            header.set_sensitive(
                target.credential.as_ref().is_some_and(|credential| credential.name == name),
            );
            result.insert(key(&name)?, header);
        }
    }
    Ok(result)
}
fn response_metadata(
    metadata: &MetadataMap,
) -> Result<BTreeMap<String, Vec<GrpcMetadataValue>>, GrpcError> {
    let mut result: BTreeMap<String, Vec<GrpcMetadataValue>> = BTreeMap::new();
    for item in metadata.iter() {
        let (name, value) = match item {
            KeyAndValueRef::Ascii(name, value) => (
                name.as_str(),
                match value.to_str() {
                    Ok(value) => GrpcMetadataValue::Ascii(value.into()),
                    Err(_) => GrpcMetadataValue::Base64(STANDARD.encode(value.as_encoded_bytes())),
                },
            ),
            KeyAndValueRef::Binary(name, value) => (
                name.as_str(),
                GrpcMetadataValue::Base64(
                    STANDARD.encode(value.to_bytes().map_err(|_| GrpcError::InvalidResponse)?),
                ),
            ),
        };
        result.entry(name.into()).or_default().push(value);
    }
    Ok(result)
}
pub(super) async fn execute(
    schema: &CompiledSchema,
    target: &GrpcTarget<'_>,
    action: GrpcAction<'_>,
    budget: GrpcBudget,
) -> Result<GrpcReport, GrpcError> {
    if !(1..=600_000).contains(&budget.timeout_ms) || !(1..=16_777_216).contains(&budget.max_bytes)
    {
        return Err(GrpcError::InvalidBudget);
    }
    endpoint(target)?;
    let started = Instant::now();
    let duration = Duration::from_millis(budget.timeout_ms);
    let report = tokio::time::timeout(duration, async {
        let call = match action {
            GrpcAction::Test => None,
            GrpcAction::Call(call) => Some(call),
        };
        let metadata = request_metadata(target, call)?;
        let prepared = if let Some(call) = call {
            let method = schema.method(&call.method)?;
            let serialized = call.message.to_string();
            let mut deserializer = serde_json::Deserializer::from_str(&serialized);
            let message = DynamicMessage::deserialize(method.input(), &mut deserializer)
                .map_err(|_| GrpcError::InvalidMessage)?;
            if message.encoded_len() > budget.max_bytes {
                return Err(GrpcError::OutputLimit);
            }
            Some((method, message))
        } else {
            None
        };
        let (channel, _socket) = transport::connect(target, duration).await?;
        let Some((method, message)) = prepared else {
            return Ok(GrpcReport::Connected { elapsed_ms: started.elapsed().as_millis() as u64 });
        };
        let path = format!("/{}/{}", method.parent_service().full_name(), method.name());
        let mut client = tonic::client::Grpc::new(channel)
            .max_decoding_message_size(budget.max_bytes)
            .max_encoding_message_size(budget.max_bytes);
        client.ready().await?;
        let mut request = Request::new(message);
        *request.metadata_mut() = metadata;
        request.set_timeout(duration);
        let response = client
            .unary(
                request,
                path.parse().map_err(|_| GrpcError::InvalidMessage)?,
                codec::DynamicCodec(method.output()),
            )
            .await;
        let (status, response, metadata) = match response {
            Ok(response) => {
                let metadata = response_metadata(response.metadata())?;
                let value = serde_json::to_value(response.into_inner())
                    .map_err(|_| GrpcError::InvalidResponse)?;
                (GrpcStatus { code: 0, message: "OK".into(), details: None }, Some(value), metadata)
            }
            Err(status) => {
                let metadata = response_metadata(status.metadata())?;
                let details = if status.details().is_empty() {
                    None
                } else {
                    Some(STANDARD.encode(status.details()))
                };
                (
                    GrpcStatus {
                        code: status.code() as i32,
                        message: status.message().into(),
                        details,
                    },
                    None,
                    metadata,
                )
            }
        };
        Ok::<_, GrpcError>(GrpcReport::Called {
            method: path,
            status,
            response,
            metadata,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    })
    .await
    .map_err(|_| GrpcError::Timeout)??;
    if serde_json::to_vec(&report).map_err(|_| GrpcError::OutputLimit)?.len() > budget.max_bytes {
        return Err(GrpcError::OutputLimit);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_credentials_are_sensitive_and_cannot_be_overridden_by_public_headers() {
        let metadata = BTreeMap::new();
        let target = GrpcTarget {
            endpoint: "https://localhost",
            server_name: None,
            ca_certificate: None,
            identity: None,
            metadata: &metadata,
            credential: Some(GrpcCredential {
                name: "authorization",
                value: "Bearer private-token",
            }),
        };
        let headers = request_metadata(&target, None).expect("metadata");
        assert!(headers.get("authorization").expect("credential").is_sensitive());
        assert!(validate_metadata(&[("authorization".into(), "plaintext".into())].into(), None)
            .is_err());
        assert!(validate_endpoint("http://localhost", true).is_err());
        assert!(validate_endpoint("https://user:password@localhost", false).is_err());
    }
}
