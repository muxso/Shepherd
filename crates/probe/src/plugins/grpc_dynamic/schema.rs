use prost_reflect::{DescriptorPool, DynamicMessage, MethodDescriptor, SerializeOptions};
use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error(
        "invalid protobuf files; provide up to 32 relative .proto files, at most 256 KiB total"
    )]
    InvalidFiles,
    #[error("invalid protobuf definition: {0}")]
    Definition(#[from] protox::Error),
    #[error("standard protobuf namespaces cannot be redefined")]
    ReservedNamespace,
    #[error("provide at least one service and no more than 256 methods")]
    MethodLimit,
    #[error("unknown gRPC method")]
    UnknownMethod,
    #[error("streaming gRPC methods are unsupported")]
    Streaming,
}
struct MemoryFiles(BTreeMap<String, String>);
impl FileResolver for MemoryFiles {
    fn resolve_path(&self, path: &Path) -> Option<String> {
        path.to_str().filter(|name| self.0.contains_key(*name)).map(str::to_owned)
    }
    fn open_file(&self, name: &str) -> Result<File, protox::Error> {
        self.0
            .get(name)
            .map(|source| File::from_source(name, source))
            .unwrap_or_else(|| Err(protox::Error::file_not_found(name)))
    }
}
#[derive(Clone)]
pub struct CompiledSchema {
    pool: DescriptorPool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodInfo {
    pub path: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_example: Option<Value>,
}
impl CompiledSchema {
    pub fn compile(files: &BTreeMap<String, String>) -> Result<Self, SchemaError> {
        if files.is_empty()
            || files.len() > 32
            || files.values().map(String::len).sum::<usize>() > 262_144
            || files.keys().any(|name| {
                name.len() > 256
                    || !name.ends_with(".proto")
                    || name.starts_with("google/protobuf/")
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"/_.-".contains(&byte))
                    || name.split('/').any(|part| part.is_empty() || matches!(part, "." | ".."))
            })
        {
            return Err(SchemaError::InvalidFiles);
        }
        let mut resolver = ChainFileResolver::new();
        resolver.add(MemoryFiles(files.clone()));
        resolver.add(GoogleFileResolver::new());
        let mut compiler = protox::Compiler::with_file_resolver(resolver);
        compiler.open_files(files.keys())?;
        let pool = compiler.descriptor_pool();
        if pool.files().any(|file| {
            (file.package_name() == "google.protobuf"
                || file.package_name().starts_with("google.protobuf."))
                && !file.name().starts_with("google/protobuf/")
        }) {
            return Err(SchemaError::ReservedNamespace);
        }
        let methods: usize = pool.services().map(|service| service.methods().len()).sum();
        if methods == 0 || methods > 256 {
            return Err(SchemaError::MethodLimit);
        }
        Ok(Self { pool })
    }
    pub fn methods(&self) -> Vec<MethodInfo> {
        let mut methods = Vec::new();
        for service in self.pool.services() {
            for method in service.methods() {
                let message = DynamicMessage::new(method.input());
                let input_example = message
                    .serialize_with_options(
                        serde_json::value::Serializer,
                        &SerializeOptions::new().skip_default_fields(false),
                    )
                    .ok();
                methods.push(MethodInfo {
                    path: format!("/{}/{}", service.full_name(), method.name()),
                    input_type: method.input().full_name().into(),
                    output_type: method.output().full_name().into(),
                    client_streaming: method.is_client_streaming(),
                    server_streaming: method.is_server_streaming(),
                    input_example,
                });
            }
        }
        methods
    }
    pub fn method(&self, path: &str) -> Result<MethodDescriptor, SchemaError> {
        let (service, name) = path
            .strip_prefix('/')
            .unwrap_or(path)
            .split_once('/')
            .ok_or(SchemaError::UnknownMethod)?;
        let method = self
            .pool
            .get_service_by_name(service)
            .and_then(|service| service.methods().find(|method| method.name() == name))
            .ok_or(SchemaError::UnknownMethod)?;
        if method.is_client_streaming() || method.is_server_streaming() {
            return Err(SchemaError::Streaming);
        }
        Ok(method)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definitions_resolve_only_memory_and_bundled_standard_files() {
        let files=[("types.proto".into(),"syntax=\"proto3\";package fixture; import \"google/protobuf/timestamp.proto\";message Input {string value=1;google.protobuf.Timestamp time=2;}".into()),("service.proto".into(),"syntax=\"proto3\";package fixture;import \"types.proto\";service Echo {rpc Call(Input) returns (Input);rpc Stream(stream Input) returns(stream Input);}".into())].into();
        let schema = CompiledSchema::compile(&files).expect("definitions");
        assert_eq!(schema.methods().len(), 2);
        schema.method("/fixture.Echo/Call").expect("method");
        assert!(matches!(schema.method("/fixture.Echo/Stream"), Err(SchemaError::Streaming)));
        let external = tempfile::Builder::new().suffix(".proto").tempfile().expect("external file");
        std::fs::write(external.path(), "syntax=\"proto3\";message External {}").expect("fixture");
        let import =
            serde_json::to_string(external.path().to_str().expect("path")).expect("import");
        let files=[("service.proto".into(),format!("syntax=\"proto3\";import {import};message M {{}}service Echo {{rpc Call(M) returns(M);}}"))].into();
        assert!(
            CompiledSchema::compile(&files).is_err(),
            "an existing host file must never be imported"
        );
        let files=[("reserved.proto".into(),"syntax=\"proto3\";package google.protobuf;message Timestamp {}service Echo {rpc Call(Timestamp) returns(Timestamp);}".into())].into();
        assert!(matches!(CompiledSchema::compile(&files), Err(SchemaError::ReservedNamespace)));
    }
}
