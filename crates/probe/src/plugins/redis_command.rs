//! Each request owns its connection, including the background driver task.
use crate::domain::redis::{CommandError, RedisCommand, RedisReplyKind, RedisReport};
use redis::{AsyncConnectionConfig, Value};
use serde_json::{json, Value as Json};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum RedisCommandError {
    #[error(transparent)]
    InvalidRequest(#[from] CommandError),
    #[error("invalid Redis execution budget")]
    InvalidBudget,
    #[error("Redis command timed out")]
    Timeout,
    #[error("Redis result exceeds output or nesting limit")]
    OutputLimit,
    #[error("unexpected Redis push response; streaming operations are unsupported")]
    PushResponse,
    #[error("Redis command failed: {0}")]
    Driver(#[source] redis::RedisError),
}
impl From<redis::RedisError> for RedisCommandError {
    fn from(error: redis::RedisError) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            Self::Driver(error)
        }
    }
}
#[derive(Clone, Copy)]
pub struct RedisBudget {
    pub timeout_ms: u64,
    pub max_bytes: usize,
}

pub(super) async fn execute(
    target: &str,
    request: &RedisCommand,
    budget: RedisBudget,
) -> Result<RedisReport, RedisCommandError> {
    request.validate()?;
    if !(1..=600_000).contains(&budget.timeout_ms) || !(1..=16_777_216).contains(&budget.max_bytes)
    {
        return Err(RedisCommandError::InvalidBudget);
    }
    let started = Instant::now();
    let duration = Duration::from_millis(budget.timeout_ms);
    let mut report = tokio::time::timeout(duration, async {
        let client = redis::Client::open(target)?;
        let config = AsyncConnectionConfig::new()
            .set_connection_timeout(duration)
            .set_response_timeout(duration);
        let mut connection = client.get_multiplexed_async_connection_with_config(&config).await?;
        let mut command = redis::cmd(&request.command);
        for argument in &request.arguments {
            command.arg(argument.bytes()?.as_ref());
        }
        let reply: Value = command.query_async(&mut connection).await?;
        let mut decoder = Decoder { bytes: budget.max_bytes, nodes: 10_000 };
        let (kind, value) = decoder.decode(&reply, 0)?;
        Ok::<_, RedisCommandError>(RedisReport { kind, value, elapsed_ms: 0 })
    })
    .await
    .map_err(|_| RedisCommandError::Timeout)??;
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    if serde_json::to_vec(&report).map_err(|_| RedisCommandError::OutputLimit)?.len()
        > budget.max_bytes
    {
        return Err(RedisCommandError::OutputLimit);
    }
    Ok(report)
}
struct Decoder {
    bytes: usize,
    nodes: usize,
}
impl Decoder {
    fn charge(&mut self, bytes: usize) -> Result<(), RedisCommandError> {
        self.bytes = self.bytes.checked_sub(bytes).ok_or(RedisCommandError::OutputLimit)?;
        Ok(())
    }
    fn decode(
        &mut self,
        value: &Value,
        depth: usize,
    ) -> Result<(RedisReplyKind, Json), RedisCommandError> {
        use RedisReplyKind as Kind;
        if depth > 64 {
            return Err(RedisCommandError::OutputLimit);
        }
        self.nodes = self.nodes.checked_sub(1).ok_or(RedisCommandError::OutputLimit)?;
        self.charge(2)?;
        let decoded = match value {
            Value::Nil => (Kind::Null, Json::Null),
            Value::Int(value) => (Kind::Integer, json!(value.to_string())),
            Value::BigNumber(value) => (Kind::Integer, json!(value.to_string())),
            Value::Double(value) => (
                Kind::Float,
                serde_json::Number::from_f64(*value)
                    .map(Json::Number)
                    .unwrap_or_else(|| json!(value.to_string())),
            ),
            Value::Boolean(value) => (Kind::Boolean, json!(value)),
            Value::Okay => (Kind::String, json!("OK")),
            Value::SimpleString(value) => {
                self.charge(value.len())?;
                (Kind::String, json!(value))
            }
            Value::BulkString(value) => match std::str::from_utf8(value) {
                Ok(text) => {
                    self.charge(value.len())?;
                    (Kind::String, json!(text))
                }
                Err(_) => {
                    self.charge(value.len().checked_mul(2).ok_or(RedisCommandError::OutputLimit)?)?;
                    let mut hex = String::with_capacity(value.len() * 2);
                    const DIGITS: &[u8; 16] = b"0123456789abcdef";
                    for byte in value {
                        hex.push(DIGITS[(byte >> 4) as usize] as char);
                        hex.push(DIGITS[(byte & 15) as usize] as char);
                    }
                    (Kind::Binary, json!({"hex":hex}))
                }
            },
            Value::Array(items) | Value::Set(items) => {
                let kind = if matches!(value, Value::Set(_)) { Kind::Set } else { Kind::Array };
                let items = items
                    .iter()
                    .map(|value| self.decode(value, depth + 1).map(|(_, value)| value))
                    .collect::<Result<Vec<_>, _>>()?;
                (kind, Json::Array(items))
            }
            Value::Map(items) => (Kind::Map, self.pairs(items, depth + 1)?),
            Value::Attribute { data, attributes } => (
                Kind::Attribute,
                json!({"data":self.decode(data,depth+1)?.1,"attributes":self.pairs(attributes,depth+1)?}),
            ),
            Value::VerbatimString { format, text } => {
                self.charge(text.len())?;
                let format = match format {
                    redis::VerbatimFormat::Text => "txt",
                    redis::VerbatimFormat::Markdown => "mkd",
                    redis::VerbatimFormat::Unknown(format) => format,
                };
                self.charge(format.len())?;
                (Kind::Verbatim, json!({"format":format,"text":text}))
            }
            Value::Push { .. } => return Err(RedisCommandError::PushResponse),
            Value::ServerError(error) => return Err(redis::RedisError::from(error.clone()).into()),
        };
        Ok(decoded)
    }
    fn pairs(&mut self, items: &[(Value, Value)], depth: usize) -> Result<Json, RedisCommandError> {
        items
            .iter()
            .map(|(key, value)| {
                Ok(json!([self.decode(key, depth)?.1, self.decode(value, depth)?.1]))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Json::Array)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoding_preserves_binary_precision_maps_and_nulls() {
        let reply = Value::Array(vec![
            Value::Nil,
            Value::Int(9007199254740993),
            Value::BulkString(vec![0, 255]),
            Value::Map(vec![
                (Value::SimpleString("same".into()), Value::Int(1)),
                (Value::SimpleString("same".into()), Value::Int(2)),
            ]),
        ]);
        let (_, value) = Decoder { bytes: 4096, nodes: 100 }.decode(&reply, 0).expect("decode");
        assert_eq!(
            value,
            json!([null,"9007199254740993",{"hex":"00ff"},[["same","1"],["same","2"]]])
        );
        assert!(matches!(
            Decoder { bytes: 2, nodes: 100 }.decode(&Value::BulkString(vec![1; 10]), 0),
            Err(RedisCommandError::OutputLimit)
        ));
        assert!(matches!(
            Decoder { bytes: 4096, nodes: 1 }.decode(&reply, 0),
            Err(RedisCommandError::OutputLimit)
        ));
        let mut deep = Value::Nil;
        for _ in 0..66 {
            deep = Value::Array(vec![deep]);
        }
        assert!(matches!(
            Decoder { bytes: 4096, nodes: 100 }.decode(&deep, 0),
            Err(RedisCommandError::OutputLimit)
        ));
    }
}
