//! Method dispatch, and the era decision that makes this connectable.

use devto_client::{Clock, DevtoClient, Transport};
use serde_json::{Value, json};

use crate::config::Config;
use crate::protocol::{
    Era, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, MODERN_VERSION, PARSE_ERROR, Request,
    SUPPORTED_HANDSHAKE_VERSIONS, UNSUPPORTED_PROTOCOL_VERSION, error, error_with, failure,
    success,
};
use crate::tools::{self, ToolContext};

pub const SERVER_NAME: &str = "devto-mcp";

/// Sent in both the `initialize` result and the `server/discover` result — the spec asks
/// for the same string in each, and a client only ever sees one of them.
pub const INSTRUCTIONS: &str = "\
Authorship on DEV (dev.to) with the platform's own rules built in.

Start with `whoami`: it reports which account this acts for, what it is permitted to do, and
how much rate-limit budget is left. dev.to allows 30 reads per minute and 1 write per second,
counted against the IP and the API key at the same time, so plan calls rather than fanning out.

Before sending any article, run `validate_draft`. It costs no request and catches the rules
that are not in dev.to's API documentation: the body limit is in bytes not characters, the
title limit is measured with whitespace stripped, tags are letters and digits only with no
hyphens, and front matter inside body_markdown silently overrides the fields you send.

dev.to publishes obligations for automated clients at https://dev.to/llms.txt, and this server
holds you to them. Send `ai_disclosure_level` accurately on anything you write: `no_ai` means a
human wrote it without meaningful AI assistance, `some_ai` means meaningful AI assistance
including drafting or major editing, and `fully_autonomous` means an agent or model produced it
even when a human asked for it or approved it. A human reviewing generated text does not make
it human-authored. Only publish, edit or follow when the account holder has asked for that
specific action.

Three things are impossible rather than merely unimplemented: writing or replying to comments
(no endpoint exists in any API version, and the platform asks clients not to automate it),
reacting to posts (restricted to admin keys), and uploading images (cover images must already
be hosted at a public URL).";

pub struct Server<T: Transport, C: Clock> {
    config: Config,
    client: DevtoClient<T, C>,
    era: Era,
    era_locked: bool,
    now_unix: i64,
}

impl<T: Transport, C: Clock> Server<T, C> {
    pub fn new(config: Config, client: DevtoClient<T, C>, now_unix: i64) -> Self {
        Self {
            config,
            client,
            // Assumed until a client proves otherwise, because the clients that exist today
            // open with `initialize` and cannot fall forward if we guess the other way.
            era: Era::Handshake {
                version: SUPPORTED_HANDSHAKE_VERSIONS[0].to_string(),
            },
            era_locked: false,
            now_unix,
        }
    }

    #[cfg(test)]
    pub fn era(&self) -> &Era {
        &self.era
    }

    /// Handle one line of newline-delimited JSON-RPC. `None` means "say nothing", which is
    /// the correct response to a notification.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let parsed: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(e) => {
                return Some(
                    failure(None, error(PARSE_ERROR, format!("invalid JSON: {e}"))).to_string(),
                );
            }
        };

        let id = parsed.get("id").cloned();
        let request: Request = match serde_json::from_value(parsed) {
            Ok(request) => request,
            Err(e) => {
                return Some(
                    failure(id, error(INVALID_REQUEST, format!("invalid request: {e}")))
                        .to_string(),
                );
            }
        };

        self.handle(request).map(|value| value.to_string())
    }

    fn handle(&mut self, request: Request) -> Option<Value> {
        if let Some(rejection) = self.settle_era(&request) {
            return Some(rejection);
        }

        let id = request.id.clone();
        let is_notification = request.is_notification();

        let result = match request.method.as_str() {
            "initialize" => Ok(self.initialize(&request)),
            "notifications/initialized" | "notifications/cancelled" => return None,
            "server/discover" => Ok(self.discover()),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => self.call_tool(&request),
            other => Err(error(
                METHOD_NOT_FOUND,
                format!("this server does not implement {other}"),
            )),
        };

        // A notification is acted on and never answered, even if it failed.
        if is_notification {
            return None;
        }

        Some(match result {
            Ok(value) => success(id, &self.era, value),
            Err(e) => failure(id, e),
        })
    }

    /// Decide the era from how the client opened, then hold it for the process.
    ///
    /// Returns a rejection if the client asked for a version this server cannot speak.
    fn settle_era(&mut self, request: &Request) -> Option<Value> {
        if let Some(declared) = request.declared_version() {
            if declared != MODERN_VERSION {
                return Some(failure(
                    request.id.clone(),
                    error_with(
                        UNSUPPORTED_PROTOCOL_VERSION,
                        format!("unsupported protocol version {declared}"),
                        json!({ "supported": supported_versions(), "requested": declared }),
                    ),
                ));
            }
            if !self.era_locked {
                self.era = Era::Stateless;
                self.era_locked = true;
            }
            return None;
        }

        if request.method == "initialize" && !self.era_locked {
            let asked = request.param_str("protocolVersion");
            let version = match asked {
                Some(asked) if SUPPORTED_HANDSHAKE_VERSIONS.contains(&asked) => asked.to_string(),
                // An unfamiliar handshake version is answered with our newest rather than
                // refused: the client picks what to do with a version it did not ask for.
                _ => SUPPORTED_HANDSHAKE_VERSIONS[0].to_string(),
            };
            self.era = Era::Handshake { version };
            self.era_locked = true;
        }
        None
    }

    fn initialize(&self, _request: &Request) -> Value {
        let version = match &self.era {
            Era::Handshake { version } => version.clone(),
            Era::Stateless => MODERN_VERSION.to_string(),
        };
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": server_info(),
            "instructions": INSTRUCTIONS,
        })
    }

    fn discover(&self) -> Value {
        json!({
            "supportedVersions": supported_versions(),
            "capabilities": { "tools": {} },
            "serverInfo": server_info(),
            "instructions": INSTRUCTIONS,
        })
    }

    fn call_tool(&mut self, request: &Request) -> Result<Value, crate::protocol::ErrorObject> {
        let Some(name) = request.param_str("name") else {
            return Err(error(INVALID_PARAMS, "tools/call requires a tool name"));
        };
        let arguments = request
            .params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let mut ctx = ToolContext {
            client: &mut self.client,
            config: &self.config,
            now_unix: self.now_unix,
        };
        let outcome = tools::call(name, &arguments, &mut ctx);

        // The text block mirrors the structured content: clients differ in which they read,
        // and a model should never see an empty result because it looked at the other one.
        let text = serde_json::to_string_pretty(&outcome.structured)
            .unwrap_or_else(|_| outcome.structured.to_string());

        Ok(json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": outcome.structured,
            "isError": outcome.is_error,
        }))
    }
}

fn server_info() -> Value {
    json!({ "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") })
}

fn supported_versions() -> Vec<String> {
    let mut versions = vec![MODERN_VERSION.to_string()];
    versions.extend(SUPPORTED_HANDSHAKE_VERSIONS.iter().map(|v| v.to_string()));
    versions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Capabilities, Config};
    use devto_client::{HttpRequest, HttpResponse};
    use std::cell::RefCell;

    struct Offline;

    impl Transport for Offline {
        fn execute(&self, _request: HttpRequest) -> Result<HttpResponse, String> {
            Err("this test does not reach the network".to_string())
        }
    }

    struct Scripted(RefCell<Vec<HttpResponse>>);

    impl Transport for Scripted {
        fn execute(&self, _request: HttpRequest) -> Result<HttpResponse, String> {
            let mut responses = self.0.borrow_mut();
            if responses.is_empty() {
                return Err("out of scripted responses".to_string());
            }
            Ok(responses.remove(0))
        }
    }

    struct Frozen;

    impl Clock for Frozen {
        fn now_millis(&self) -> u64 {
            0
        }
        fn sleep_millis(&self, _millis: u64) {}
    }

    fn server_with<T: Transport>(api_key: Option<&str>, transport: T) -> Server<T, Frozen> {
        let config = Config {
            base_url: "https://dev.to".to_string(),
            api_key: api_key.map(str::to_string),
            capabilities: Capabilities {
                publish: false,
                claim_no_ai: false,
            },
        };
        let client = DevtoClient::with_parts(
            devto_client::Config {
                base_url: config.base_url.clone(),
                api_key: config.api_key.clone(),
                ..devto_client::Config::default()
            },
            transport,
            Frozen,
        );
        Server::new(config, client, 1_757_000_000)
    }

    fn offline() -> Server<Offline, Frozen> {
        server_with(None, Offline)
    }

    fn respond(server: &mut Server<impl Transport, Frozen>, line: &str) -> Value {
        serde_json::from_str(&server.handle_line(line).expect("expected a response")).unwrap()
    }

    // ---- era -------------------------------------------------------------------------

    /// Claude Code's opening message. If this does not work, nothing else matters.
    #[test]
    fn a_legacy_client_that_opens_with_initialize_is_answered_in_its_own_era() {
        let mut server = offline();
        let response = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize",
                "params":{"protocolVersion":"2025-11-25","clientInfo":{"name":"claude-code"}}}"#,
        );

        assert_eq!(response["result"]["protocolVersion"], json!("2025-11-25"));
        assert_eq!(response["result"]["serverInfo"]["name"], json!("devto-mcp"));
        assert!(response["result"]["capabilities"]["tools"].is_object());
        assert!(
            response["result"].get("resultType").is_none(),
            "a handshake-era client must not be sent resultType"
        );
        assert!(matches!(server.era(), Era::Handshake { .. }));
    }

    #[test]
    fn an_older_handshake_version_is_echoed_rather_than_upgraded() {
        let mut server = offline();
        let response = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize",
                "params":{"protocolVersion":"2024-11-05"}}"#,
        );
        assert_eq!(response["result"]["protocolVersion"], json!("2024-11-05"));
    }

    #[test]
    fn an_unfamiliar_handshake_version_is_answered_with_our_newest() {
        let mut server = offline();
        let response = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize",
                "params":{"protocolVersion":"1999-01-01"}}"#,
        );
        assert_eq!(response["result"]["protocolVersion"], json!("2025-11-25"));
    }

    #[test]
    fn a_stateless_client_declaring_the_current_revision_gets_result_type() {
        let mut server = offline();
        let response = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list",
                "params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
        );
        assert_eq!(response["result"]["resultType"], json!("complete"));
        assert_eq!(server.era(), &Era::Stateless);
    }

    #[test]
    fn an_unsupported_declared_version_is_refused_with_what_we_do_support() {
        let mut server = offline();
        let response = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list",
                "params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"1999-01-01"}}}"#,
        );
        assert_eq!(
            response["error"]["code"],
            json!(UNSUPPORTED_PROTOCOL_VERSION)
        );
        assert_eq!(response["error"]["data"]["requested"], json!("1999-01-01"));
        assert!(
            response["error"]["data"]["supported"]
                .as_array()
                .unwrap()
                .contains(&json!("2026-07-28"))
        );
    }

    /// The era is settled once. A client that opened with `initialize` stays in that era.
    #[test]
    fn the_era_does_not_change_after_it_is_settled() {
        let mut server = offline();
        respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#,
        );
        let later = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        assert!(later["result"].get("resultType").is_none());
        assert!(matches!(server.era(), Era::Handshake { .. }));
    }

    /// The era locks when `initialize` arrives, not on whatever message happens to be
    /// first. A client that lists tools before initializing must still get its own
    /// version echoed when it does initialize.
    #[test]
    fn an_early_request_does_not_lock_the_era_before_initialize() {
        let mut server = offline();
        respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );

        let response = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize",
                "params":{"protocolVersion":"2024-11-05"}}"#,
        );
        assert_eq!(
            response["result"]["protocolVersion"],
            json!("2024-11-05"),
            "the era was locked by a request that was not initialize"
        );
    }

    /// Settled means settled. A second `initialize` asking for a different version must
    /// not move the era out from under the conversation already in progress.
    #[test]
    fn a_second_initialize_cannot_change_the_era_already_agreed() {
        let mut server = offline();
        let first = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#,
        );
        assert_eq!(first["result"]["protocolVersion"], json!("2025-11-25"));

        let second = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
        );
        assert_eq!(
            second["result"]["protocolVersion"],
            json!("2025-11-25"),
            "the era was renegotiated mid-session"
        );
    }

    #[test]
    fn discover_and_initialize_carry_the_same_instructions() {
        let mut server = offline();
        let discovered = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"server/discover"}"#,
        );
        let initialized = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#,
        );
        assert_eq!(
            discovered["result"]["instructions"],
            initialized["result"]["instructions"]
        );
        assert!(
            discovered["result"]["supportedVersions"]
                .as_array()
                .unwrap()
                .contains(&json!(MODERN_VERSION))
        );
    }

    // ---- plumbing --------------------------------------------------------------------

    #[test]
    fn a_notification_is_acted_on_and_never_answered() {
        let mut server = offline();
        assert!(
            server
                .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .is_none()
        );
        assert!(
            server
                .handle_line(r#"{"jsonrpc":"2.0","method":"tools/list"}"#)
                .is_none(),
            "any request without an id is a notification"
        );
    }

    #[test]
    fn a_blank_line_is_ignored() {
        assert!(offline().handle_line("   ").is_none());
        assert!(offline().handle_line("").is_none());
    }

    #[test]
    fn malformed_input_is_reported_without_killing_the_session() {
        let mut server = offline();
        let parse_error = respond(&mut server, "{not json");
        assert_eq!(parse_error["error"]["code"], json!(PARSE_ERROR));

        let invalid = respond(&mut server, r#"{"jsonrpc":"2.0","id":1}"#);
        assert_eq!(invalid["error"]["code"], json!(INVALID_REQUEST));
        assert_eq!(
            invalid["id"],
            json!(1),
            "the id must survive to be matched up"
        );

        // The session keeps working.
        let ok = respond(&mut server, r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#);
        assert_eq!(ok["result"], json!({}));
    }

    #[test]
    fn an_unknown_method_is_a_method_not_found() {
        let response = respond(
            &mut offline(),
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#,
        );
        assert_eq!(response["error"]["code"], json!(METHOD_NOT_FOUND));
    }

    #[test]
    fn ping_answers_empty() {
        let response = respond(
            &mut offline(),
            r#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#,
        );
        assert_eq!(response["result"], json!({}));
        assert_eq!(response["id"], json!(9));
    }

    // ---- tools -----------------------------------------------------------------------

    #[test]
    fn tools_list_returns_every_tool_with_a_schema() {
        let response = respond(
            &mut offline(),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        let listed = response["result"]["tools"].as_array().unwrap();
        assert_eq!(listed.len(), tools::definitions().len());
        assert!(listed.iter().all(|t| t["inputSchema"].is_object()));
    }

    #[test]
    fn a_tool_call_returns_the_same_payload_as_text_and_as_structured_content() {
        let response = respond(
            &mut offline(),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"validate_draft",
                "arguments":{"title":"A title","tags":["rust"],"ai_disclosure_level":"some_ai"}}}"#,
        );
        let result = &response["result"];
        assert_eq!(result["isError"], json!(false));
        assert_eq!(result["structuredContent"]["sendable"], json!(true));

        let text = result["content"][0]["text"].as_str().unwrap();
        let reparsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(reparsed, result["structuredContent"]);
    }

    /// Business failures come back as a tool result, not a JSON-RPC error, so the client
    /// hands them to the model to fix rather than treating the call as broken.
    #[test]
    fn a_tool_failure_is_a_result_not_a_protocol_error() {
        let response = respond(
            &mut offline(),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"my_articles","arguments":{}}}"#,
        );
        assert!(response.get("error").is_none());
        assert_eq!(response["result"]["isError"], json!(true));
        assert!(
            response["result"]["structuredContent"]["remedy"]
                .as_str()
                .unwrap()
                .contains("DEVTO_API_KEY")
        );
    }

    #[test]
    fn calling_a_tool_that_does_not_exist_names_the_ones_that_do() {
        let response = respond(
            &mut offline(),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"no_such_tool"}}"#,
        );
        assert_eq!(response["result"]["isError"], json!(true));
        let remedy = response["result"]["structuredContent"]["remedy"]
            .as_str()
            .unwrap();
        assert!(remedy.contains("validate_draft"), "{remedy}");
    }

    #[test]
    fn tools_call_without_a_name_is_a_parameter_error() {
        let response = respond(
            &mut offline(),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{}}"#,
        );
        assert_eq!(response["error"]["code"], json!(INVALID_PARAMS));
    }

    #[test]
    fn whoami_reports_the_walls_even_without_a_key() {
        let response = respond(
            &mut offline(),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"whoami"}}"#,
        );
        let content = &response["result"]["structuredContent"];
        assert_eq!(content["authenticated"], json!(false));
        assert_eq!(content["account"], Value::Null);
        assert_eq!(content["capabilities"]["publish"], json!(false));
        assert_eq!(content["capabilities"]["comment"], json!(false));
        assert_eq!(content["capabilities"]["react"], json!(false));
        assert_eq!(content["budget_remaining"]["reads_this_minute"], json!(30));
    }

    #[test]
    fn whoami_reports_the_account_when_a_key_is_present() {
        let scripted = Scripted(RefCell::new(vec![HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: r#"{"id":965504,"username":"copyleftdev","name":"Don Johnson"}"#.to_string(),
        }]));
        let mut server = server_with(Some("secret"), scripted);
        let response = respond(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"whoami"}}"#,
        );
        let content = &response["result"]["structuredContent"];
        assert_eq!(content["authenticated"], json!(true));
        assert_eq!(content["account"]["username"], json!("copyleftdev"));
        assert_eq!(
            content["capabilities"]["publish"],
            json!(false),
            "a key alone must not grant publishing"
        );
    }

    /// The governance text is the point of the server, not decoration.
    #[test]
    fn the_instructions_state_the_obligations_and_the_walls() {
        assert!(INSTRUCTIONS.contains("llms.txt"));
        assert!(INSTRUCTIONS.contains("ai_disclosure_level"));
        assert!(INSTRUCTIONS.contains("fully_autonomous"));
        assert!(INSTRUCTIONS.contains("validate_draft"));
        assert!(INSTRUCTIONS.contains("30 reads per minute"));
        assert!(INSTRUCTIONS.contains("account holder"));
    }
}
