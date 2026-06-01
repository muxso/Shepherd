//! mcp —— Model Context Protocol 服务端引擎(协议层)
//!
//! 处理 JSON-RPC 2.0 消息与 MCP 方法(`initialize` / `tools/list` / `tools/call` / `ping`),
//! 维护一个**通用工具注册表**(name + description + JSON Schema + 异步处理器)。本 crate
//! 不认识任何业务概念;Shepherd 的工具由组装根注册,桥接到各上下文服务。
//!
//! 约定:
//! - 协议级错误(未知方法、缺参、未知工具)→ JSON-RPC `error`;
//! - 工具执行错误 → 成功的 `result` 但 `isError: true`(MCP 惯例,让模型看见错误内容);
//! - 通知(无 `id`)→ 不产生响应(返回 None)。

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

/// MCP 支持的协议版本(客户端未声明时的默认值)。
const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

/// 工具处理器:接收 JSON 参数,返回 JSON 结果或错误信息(错误会以 isError 结果回给模型)。
#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn call(&self, args: Value) -> Result<Value, String>;
}

/// 调用方能力检查(由组装根用会话权限实现)。协议层不认识具体 RBAC 语义,只问"是否允许"。
/// `Send + Sync`:`&dyn CapabilityChecker` 需可跨 `.await` 传递(handler future 须 Send)。
pub trait CapabilityChecker: Send + Sync {
    fn allows(&self, resource: &str, action: &str) -> bool;
}

/// 放行一切(无鉴权场景/测试)。
pub struct AllowAll;
impl CapabilityChecker for AllowAll {
    fn allows(&self, _resource: &str, _action: &str) -> bool {
        true
    }
}

/// 一个已注册的工具。
pub struct Tool {
    pub name: String,
    pub description: String,
    /// 入参的 JSON Schema。
    pub input_schema: Value,
    pub handler: Arc<dyn ToolHandler>,
    /// 调用所需能力 (resource, action);None 表示无需特定权限。
    pub required: Option<(String, String)>,
}

impl Tool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        handler: Arc<dyn ToolHandler>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            handler,
            required: None,
        }
    }

    /// 声明调用此工具所需能力(resource:action)。
    pub fn requires(mut self, resource: impl Into<String>, action: impl Into<String>) -> Self {
        self.required = Some((resource.into(), action.into()));
        self
    }

    fn allowed_by(&self, checker: &dyn CapabilityChecker) -> bool {
        match &self.required {
            Some((r, a)) => checker.allows(r, a),
            None => true,
        }
    }
}

/// MCP 服务端:工具注册表 + JSON-RPC 分发。
pub struct McpServer {
    server_name: String,
    server_version: String,
    tools: Vec<Tool>,
}

impl McpServer {
    pub fn new(server_name: impl Into<String>, server_version: impl Into<String>) -> Self {
        Self { server_name: server_name.into(), server_version: server_version.into(), tools: Vec::new() }
    }

    pub fn tool(mut self, tool: Tool) -> Self {
        self.tools.push(tool);
        self
    }

    /// 处理一个 JSON-RPC 消息(`checker` 决定哪些工具可见/可调)。通知(无 `id`)返回 `None`。
    pub async fn dispatch(&self, message: Value, checker: &dyn CapabilityChecker) -> Option<Value> {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();

        let result: Result<Value, (i64, String)> = match method.as_str() {
            "initialize" => {
                let pv = message
                    .get("params")
                    .and_then(|p| p.get("protocolVersion"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(DEFAULT_PROTOCOL_VERSION)
                    .to_string();
                Ok(self.initialize_result(&pv))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.tools_list(checker)),
            "tools/call" => self.tools_call(message.get("params"), checker).await,
            _ => Err((-32601, format!("method not found: {method}"))),
        };

        // 通知不回响应。
        let id = id?;
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, msg)) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
            }
        })
    }

    fn initialize_result(&self, protocol_version: &str) -> Value {
        json!({
            "protocolVersion": protocol_version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": self.server_name, "version": self.server_version }
        })
    }

    fn tools_list(&self, checker: &dyn CapabilityChecker) -> Value {
        // 只列出调用方有权调用的工具(模型只看见能用的)。
        let tools: Vec<Value> = self
            .tools
            .iter()
            .filter(|t| t.allowed_by(checker))
            .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.input_schema }))
            .collect();
        json!({ "tools": tools })
    }

    async fn tools_call(
        &self,
        params: Option<&Value>,
        checker: &dyn CapabilityChecker,
    ) -> Result<Value, (i64, String)> {
        let params = params.ok_or((-32602, "missing params".to_string()))?;
        let name = params
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or((-32602, "missing tool name".to_string()))?;
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or((-32602, format!("unknown tool: {name}")))?;

        // 按工具 RBAC:权限不足 → JSON-RPC 错误(-32003)。
        if !tool.allowed_by(checker) {
            return Err((-32003, format!("permission denied for tool: {name}")));
        }

        // 工具执行错误以 isError 结果回给模型(非 JSON-RPC error)。
        Ok(match tool.handler.call(args).await {
            Ok(v) => json!({
                "content": [{ "type": "text", "text": serde_json::to_string(&v).unwrap_or_default() }],
                "isError": false
            }),
            Err(e) => json!({
                "content": [{ "type": "text", "text": e }],
                "isError": true
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;
    #[async_trait]
    impl ToolHandler for EchoTool {
        async fn call(&self, args: Value) -> Result<Value, String> {
            if args.get("fail").and_then(|f| f.as_bool()).unwrap_or(false) {
                return Err("boom".to_string());
            }
            Ok(json!({ "echo": args }))
        }
    }

    fn server() -> McpServer {
        McpServer::new("shepherd", "0.1").tool(Tool::new(
            "echo",
            "echo back arguments",
            json!({ "type": "object" }),
            Arc::new(EchoTool),
        ))
    }

    #[tokio::test]
    async fn initialize_echoes_protocol_version_and_serverinfo() {
        let req = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}});
        let resp = server().dispatch(req, &AllowAll).await.expect("response");
        assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(resp["result"]["serverInfo"]["name"], "shepherd");
        assert_eq!(resp["id"], 1);
    }

    #[tokio::test]
    async fn tools_list_returns_registered() {
        let resp = server().dispatch(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), &AllowAll).await.expect("r");
        let tools = resp["result"]["tools"].as_array().expect("arr");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "echo");
        assert!(tools[0]["inputSchema"].is_object());
    }

    #[tokio::test]
    async fn tools_call_success_wraps_text_content() {
        let req = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"x":1}}});
        let resp = server().dispatch(req, &AllowAll).await.expect("r");
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().expect("text");
        let parsed: Value = serde_json::from_str(text).expect("json");
        assert_eq!(parsed["echo"]["x"], 1);
    }

    #[tokio::test]
    async fn tool_handler_error_is_iserror_result_not_jsonrpc_error() {
        let req = json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"echo","arguments":{"fail":true}}});
        let resp = server().dispatch(req, &AllowAll).await.expect("r");
        assert!(resp.get("error").is_none());
        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(resp["result"]["content"][0]["text"], "boom");
    }

    #[tokio::test]
    async fn unknown_tool_and_method_are_jsonrpc_errors() {
        let r1 = server().dispatch(json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope"}}), &AllowAll).await.expect("r");
        assert_eq!(r1["error"]["code"], -32602);
        let r2 = server().dispatch(json!({"jsonrpc":"2.0","id":6,"method":"frobnicate"}), &AllowAll).await.expect("r");
        assert_eq!(r2["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn notification_without_id_yields_no_response() {
        let resp = server().dispatch(json!({"jsonrpc":"2.0","method":"notifications/initialized"}), &AllowAll).await;
        assert!(resp.is_none());
    }

    struct Denies(&'static str); // 拒绝某 resource
    impl CapabilityChecker for Denies {
        fn allows(&self, resource: &str, _action: &str) -> bool {
            resource != self.0
        }
    }

    fn guarded_server() -> McpServer {
        McpServer::new("shepherd", "0.1").tool(
            Tool::new("secret", "guarded", json!({"type":"object"}), Arc::new(EchoTool))
                .requires("SECRET", "READ"),
        )
    }

    #[tokio::test]
    async fn tools_list_hides_disallowed_tools() {
        let allowed = guarded_server().dispatch(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}), &AllowAll).await.expect("r");
        assert_eq!(allowed["result"]["tools"].as_array().expect("a").len(), 1);
        let denied = guarded_server().dispatch(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}), &Denies("SECRET")).await.expect("r");
        assert_eq!(denied["result"]["tools"].as_array().expect("a").len(), 0); // 被过滤
    }

    #[tokio::test]
    async fn tools_call_denied_without_capability() {
        let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"secret","arguments":{}}});
        let resp = guarded_server().dispatch(req, &Denies("SECRET")).await.expect("r");
        assert_eq!(resp["error"]["code"], -32003);
    }
}
