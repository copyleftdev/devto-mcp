//! JSON-RPC plumbing and the dual-era decision.
//!
//! MCP revision `2026-07-28` removed the `initialize` handshake: the protocol version is
//! declared per request in `_meta`, `server/discover` is mandatory, and every result
//! carries a `resultType`. Shipping only that shape does not work today — Claude Code
//! 2.1.x opens with `initialize` at `2025-11-25` and has no way to fall forward, so a
//! spec-perfect modern-only server simply fails to connect.
//!
//! So the era is decided by how the client opens, and held for the life of the process.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The revision this server prefers when it gets to choose.
pub const MODERN_VERSION: &str = "2026-07-28";

/// Handshake-era revisions we will echo back to a client that asks for one.
pub const SUPPORTED_HANDSHAKE_VERSIONS: [&str; 4] =
    ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Where a stateless-era client puts the protocol version.
pub const PROTOCOL_VERSION_META_KEY: &str = "io.modelcontextprotocol/protocolVersion";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Era {
    /// The client opened with `initialize`. No `resultType`, and `ping` answers `{}`.
    Handshake { version: String },
    /// The client declared its version per request. Results carry `resultType`.
    Stateless,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    /// Some clients put `_meta` at the top level rather than inside `params`.
    #[serde(default, rename = "_meta")]
    pub meta: Option<Value>,
}

impl Request {
    /// A request with no id is a notification: it is acted on and never answered.
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// The protocol version a stateless-era client declares, wherever it put it.
    pub fn declared_version(&self) -> Option<String> {
        let from_params = self
            .params
            .get("_meta")
            .and_then(|m| m.get(PROTOCOL_VERSION_META_KEY));
        let from_top = self
            .meta
            .as_ref()
            .and_then(|m| m.get(PROTOCOL_VERSION_META_KEY));
        from_params
            .or(from_top)
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    pub fn param_str(&self, key: &str) -> Option<&str> {
        self.params.get(key).and_then(Value::as_str)
    }
}

// Standard JSON-RPC codes, plus the three the MCP spec reserves for itself.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

#[derive(Debug, Clone, Serialize)]
pub struct ErrorObject {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Build a JSON-RPC success envelope.
///
/// `resultType` is added only in the stateless era. A handshake-era client does not know
/// the field and some will reject a result carrying it.
pub fn success(id: Option<Value>, era: &Era, mut result: Value) -> Value {
    if matches!(era, Era::Stateless)
        && let Some(object) = result.as_object_mut()
    {
        object.insert("resultType".to_string(), json!("complete"));
    }
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

pub fn failure(id: Option<Value>, error: ErrorObject) -> Value {
    let mut payload = json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": error.code,
            "message": error.message,
        }
    });
    if let Some(data) = error.data {
        payload["error"]["data"] = data;
    }
    payload
}

pub fn error(code: i64, message: impl Into<String>) -> ErrorObject {
    ErrorObject {
        code,
        message: message.into(),
        data: None,
    }
}

pub fn error_with(code: i64, message: impl Into<String>, data: Value) -> ErrorObject {
    ErrorObject {
        code,
        message: message.into(),
        data: Some(data),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Request {
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn a_request_without_an_id_is_a_notification() {
        assert!(
            parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_notification()
        );
        assert!(!parse(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).is_notification());
        assert!(
            !parse(r#"{"jsonrpc":"2.0","id":"abc","method":"ping"}"#).is_notification(),
            "a string id is still an id"
        );
    }

    #[test]
    fn the_declared_version_is_found_in_params_or_at_the_top_level() {
        let in_params = parse(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list",
                "params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
        );
        assert_eq!(in_params.declared_version().as_deref(), Some("2026-07-28"));

        let at_top = parse(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list",
                "_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}"#,
        );
        assert_eq!(at_top.declared_version().as_deref(), Some("2026-07-28"));
    }

    /// Claude Code's opening request. It carries no `_meta` at all, which is precisely why
    /// a modern-only server never gets past the first message.
    #[test]
    fn a_handshake_client_declares_no_version() {
        let legacy = parse(
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize",
                "params":{"protocolVersion":"2025-11-25","clientInfo":{"name":"claude-code"}}}"#,
        );
        assert_eq!(legacy.declared_version(), None);
    }

    #[test]
    fn result_type_is_added_only_in_the_stateless_era() {
        let stateless = success(Some(json!(1)), &Era::Stateless, json!({"ok": true}));
        assert_eq!(stateless["result"]["resultType"], json!("complete"));

        let handshake = success(
            Some(json!(1)),
            &Era::Handshake {
                version: "2025-11-25".to_string(),
            },
            json!({"ok": true}),
        );
        assert_eq!(handshake["result"]["resultType"], Value::Null);
        assert_eq!(handshake["result"]["ok"], json!(true));
    }

    /// `ping` answers with an empty object, which has no room for a `resultType`. The
    /// envelope must not choke on a result that is not an object either.
    #[test]
    fn a_non_object_result_is_passed_through_untouched() {
        let response = success(Some(json!(1)), &Era::Stateless, json!([1, 2, 3]));
        assert_eq!(response["result"], json!([1, 2, 3]));
    }

    #[test]
    fn a_missing_id_serializes_as_null_rather_than_being_dropped() {
        let response = success(None, &Era::Stateless, json!({}));
        assert_eq!(response["id"], Value::Null);
        assert!(response.get("id").is_some());
    }

    #[test]
    fn an_error_carries_its_code_and_optional_data() {
        let plain = failure(Some(json!(7)), error(METHOD_NOT_FOUND, "no such method"));
        assert_eq!(plain["error"]["code"], json!(METHOD_NOT_FOUND));
        assert_eq!(plain["error"]["message"], json!("no such method"));
        assert_eq!(plain["error"].get("data"), None);

        let detailed = failure(
            Some(json!(7)),
            error_with(
                UNSUPPORTED_PROTOCOL_VERSION,
                "unsupported",
                json!({"supported": ["2026-07-28"], "requested": "1999-01-01"}),
            ),
        );
        assert_eq!(detailed["error"]["data"]["requested"], json!("1999-01-01"));
    }

    /// JSON-RPC codes are negative by specification. A dropped sign turns a well-formed
    /// error into a meaningless one that no client will recognise.
    #[test]
    fn the_error_codes_are_the_numbers_the_specifications_assign() {
        assert_eq!(PARSE_ERROR, -32700);
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(
            UNSUPPORTED_PROTOCOL_VERSION, -32022,
            "reserved by MCP, inside the -32020..-32099 band the spec keeps for itself"
        );
        for code in [
            PARSE_ERROR,
            INVALID_REQUEST,
            METHOD_NOT_FOUND,
            INVALID_PARAMS,
            UNSUPPORTED_PROTOCOL_VERSION,
        ] {
            assert!(code < 0, "{code} should be negative");
        }
    }

    #[test]
    fn a_string_parameter_is_read_only_when_it_is_a_string() {
        let request = parse(r#"{"jsonrpc":"2.0","id":1,"method":"x","params":{"a":"b","c":3}}"#);
        assert_eq!(request.param_str("a"), Some("b"));
        assert_eq!(request.param_str("c"), None);
        assert_eq!(request.param_str("missing"), None);
    }
}
