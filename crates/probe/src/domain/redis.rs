//! Structured Redis commands preserve argument boundaries and binary data.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use thiserror::Error;

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum RedisArgument {
    Text(String),
    Binary { hex: String },
}
impl RedisArgument {
    fn size(&self) -> Result<usize, CommandError> {
        let size = match self {
            Self::Text(text) => text.len(),
            Self::Binary { hex } => {
                if !hex.len().is_multiple_of(2) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(CommandError::InvalidArgument);
                }
                hex.len() / 2
            }
        };
        if size > 65_536 {
            return Err(CommandError::InvalidArgument);
        }
        Ok(size)
    }
    pub fn bytes(&self) -> Result<Cow<'_, [u8]>, CommandError> {
        self.size()?;
        match self {
            Self::Text(text) => Ok(Cow::Borrowed(text.as_bytes())),
            Self::Binary { hex } => {
                let bytes = hex
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| {
                        let high =
                            (pair[0] as char).to_digit(16).ok_or(CommandError::InvalidArgument)?;
                        let low =
                            (pair[1] as char).to_digit(16).ok_or(CommandError::InvalidArgument)?;
                        Ok((high * 16 + low) as u8)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Cow::Owned(bytes))
            }
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedisCommand {
    pub command: String,
    #[serde(default)]
    pub arguments: Vec<RedisArgument>,
}
#[derive(Debug, Error)]
pub enum CommandError {
    #[error("invalid Redis command name")]
    InvalidCommand,
    #[error("invalid or oversized Redis argument")]
    InvalidArgument,
    #[error("Redis command input exceeds limit")]
    InputLimit,
    #[error("connection-scoped Redis commands are unsupported; configure authentication, database, and protocol in the profile")]
    ConnectionMode,
}
impl RedisCommand {
    pub fn validate(&self) -> Result<(), CommandError> {
        if self.command.is_empty()
            || self.command.len() > 64
            || !self
                .command
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(CommandError::InvalidCommand);
        }
        if matches!(
            self.command.to_ascii_uppercase().as_str(),
            "AUTH"
                | "SELECT"
                | "HELLO"
                | "RESET"
                | "QUIT"
                | "MULTI"
                | "EXEC"
                | "DISCARD"
                | "WATCH"
                | "UNWATCH"
                | "SUBSCRIBE"
                | "PSUBSCRIBE"
                | "SSUBSCRIBE"
                | "UNSUBSCRIBE"
                | "PUNSUBSCRIBE"
                | "SUNSUBSCRIBE"
                | "MONITOR"
                | "SYNC"
                | "PSYNC"
                | "REPLCONF"
                | "CLIENT"
                | "READONLY"
                | "READWRITE"
                | "ASKING"
        ) {
            return Err(CommandError::ConnectionMode);
        }
        if self.arguments.len() > 256 {
            return Err(CommandError::InputLimit);
        }
        let bytes = self
            .arguments
            .iter()
            .try_fold(0usize, |bytes, argument| argument.size().map(|size| bytes + size))?;
        if bytes > 1_048_576 {
            return Err(CommandError::InputLimit);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedisReplyKind {
    Null,
    Integer,
    String,
    Binary,
    Array,
    Map,
    Set,
    Float,
    Boolean,
    Verbatim,
    Attribute,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisReport {
    pub kind: RedisReplyKind,
    pub value: Value,
    pub elapsed_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn command_validation_preserves_empty_spaced_and_binary_arguments() {
        let command: RedisCommand = serde_json::from_value(
            json!({"command":"SET","arguments":["key with spaces","",{"hex":"00ff"}]}),
        )
        .expect("command");
        command.validate().expect("valid command");
        assert_eq!(command.arguments[0].bytes().expect("text").as_ref(), b"key with spaces");
        assert!(command.arguments[1].bytes().expect("empty").is_empty());
        assert_eq!(command.arguments[2].bytes().expect("binary").as_ref(), &[0, 255]);
        for body in [
            json!({"command":"GET key"}),
            json!({"command":"GET","ignored":true}),
            json!({"command":"SET","arguments":[{"hex":"0xz1"}]}),
            json!({"command":"SET","arguments":[{"hex":"f"}]}),
        ] {
            let valid = serde_json::from_value::<RedisCommand>(body)
                .is_ok_and(|command| command.validate().is_ok());
            assert!(!valid);
        }
        for command in ["subscribe", "MULTI", "SELECT", "HELLO", "CLIENT"] {
            assert!(RedisCommand { command: command.into(), arguments: vec![] }
                .validate()
                .is_err());
        }
    }
}
