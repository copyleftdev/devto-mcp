//! The request pipeline.
//!
//! Every call goes through one path, because the things that must never be forgotten —
//! the version header, the budget, the V0 downgrade check — are only reliable if there is
//! exactly one place they can be forgotten.

use crate::cache::{ResponseCache, Ttl};
use crate::error::{Error, Result, message_from_body, parse_retry_after};
use crate::limiter::{Budget, Decision, RateLimiter, RequestKind};
use crate::models::*;
use crate::net::{SystemClock, UreqTransport};
use crate::transport::{Clock, HttpRequest, HttpResponse, Method, Transport};

/// The media type that selects API V1. Without it the same URL serves the deprecated V0
/// controller, which does not route unpublish, semantic search, reactions or surveys.
pub const V1_ACCEPT: &str = "application/vnd.forem.api-v1+json";

pub const DEFAULT_BASE_URL: &str = "https://dev.to";

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub api_key: Option<String>,
    /// dev.to's llms.txt asks automated clients to identify themselves accurately.
    pub user_agent: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key: None,
            user_agent: format!(
                "devto-mcp/{} (+https://github.com/copyleftdev/devto-mcp)",
                env!("CARGO_PKG_VERSION")
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArticleQuery<'a> {
    pub tag: Option<&'a str>,
    pub tags: Option<&'a str>,
    pub tags_exclude: Option<&'a str>,
    pub username: Option<&'a str>,
    pub state: Option<&'a str>,
    pub top: Option<u32>,
    pub collection_id: Option<i64>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MyArticleStatus {
    Published,
    Unpublished,
    All,
}

impl MyArticleStatus {
    fn path_segment(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Unpublished => "unpublished",
            Self::All => "all",
        }
    }
}

pub struct DevtoClient<T: Transport = UreqTransport, C: Clock = SystemClock> {
    config: Config,
    transport: T,
    clock: C,
    limiter: RateLimiter,
    cache: ResponseCache,
}

impl DevtoClient<UreqTransport, SystemClock> {
    pub fn new(config: Config) -> Self {
        Self::with_parts(config, UreqTransport::new(), SystemClock)
    }
}

impl<T: Transport, C: Clock> DevtoClient<T, C> {
    pub fn with_parts(config: Config, transport: T, clock: C) -> Self {
        Self {
            config,
            transport,
            clock,
            limiter: RateLimiter::new(),
            cache: ResponseCache::new(),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.config.api_key.is_some()
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// What is left of the read and write budgets right now. Worth surfacing to a caller:
    /// a model that knows it has four reads left this minute can plan instead of failing.
    pub fn budget(&self) -> Budget {
        self.limiter.budget(self.clock.now_millis())
    }

    /// The transport this client was built with. Useful to a caller that supplied its own
    /// and wants to inspect what was asked of it.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn cache_stats(&self) -> (u64, u64) {
        (self.cache.hits, self.cache.misses)
    }

    // ---- reads -----------------------------------------------------------------------

    pub fn me(&mut self) -> Result<Me> {
        self.get_json("/api/users/me", Ttl::SESSION, "users/me")
    }

    pub fn my_articles(
        &mut self,
        status: MyArticleStatus,
        page: Option<u32>,
        per_page: Option<u32>,
    ) -> Result<Vec<MyArticle>> {
        let mut query = QueryString::new();
        query.push_opt("page", page);
        query.push_opt("per_page", per_page);
        let path = format!(
            "/api/articles/me/{}{}",
            status.path_segment(),
            query.finish()
        );
        self.get_json(&path, Ttl::MINUTE, "articles/me")
    }

    pub fn article(&mut self, id: i64) -> Result<ArticleDetail> {
        self.get_json(&format!("/api/articles/{id}"), Ttl::MINUTE, "article")
    }

    pub fn article_by_path(&mut self, username: &str, slug: &str) -> Result<ArticleDetail> {
        let path = format!("/api/articles/{}/{}", encode(username), encode(slug));
        self.get_json(&path, Ttl::MINUTE, "article")
    }

    pub fn articles(&mut self, query: ArticleQuery<'_>) -> Result<Vec<ArticleSummary>> {
        let mut qs = QueryString::new();
        qs.push_opt_str("tag", query.tag);
        qs.push_opt_str("tags", query.tags);
        qs.push_opt_str("tags_exclude", query.tags_exclude);
        qs.push_opt_str("username", query.username);
        qs.push_opt_str("state", query.state);
        qs.push_opt("top", query.top);
        qs.push_opt("collection_id", query.collection_id);
        qs.push_opt("page", query.page);
        qs.push_opt("per_page", query.per_page);
        let path = format!("/api/articles{}", qs.finish());
        self.get_json(&path, Ttl::MINUTE, "articles")
    }

    pub fn search_articles(
        &mut self,
        q: &str,
        page: Option<u32>,
        per_page: Option<u32>,
    ) -> Result<Vec<ArticleSummary>> {
        let mut qs = QueryString::new();
        qs.push_str("q", q);
        qs.push_opt("page", page);
        qs.push_opt("per_page", per_page);
        let path = format!("/api/articles/search{}", qs.finish());
        self.get_json(&path, Ttl::MINUTE, "articles/search")
    }

    /// Semantic search. V1 and authenticated only, and it returns a different shape from
    /// every other article endpoint — see [`SemanticHit`].
    pub fn semantic_search(
        &mut self,
        q: &str,
        page: Option<u32>,
        per_page: Option<u32>,
        threshold: Option<f64>,
    ) -> Result<Vec<SemanticHit>> {
        let mut qs = QueryString::new();
        qs.push_str("q", q);
        qs.push_opt("page", page);
        qs.push_opt("per_page", per_page);
        if let Some(t) = threshold {
            qs.push_str("threshold", &t.to_string());
        }
        let path = format!("/api/articles/semantic_search{}", qs.finish());
        self.get_json(&path, Ttl::MINUTE, "articles/semantic_search")
    }

    pub fn tags(&mut self, page: Option<u32>, per_page: Option<u32>) -> Result<Vec<Tag>> {
        let mut qs = QueryString::new();
        qs.push_opt("page", page);
        qs.push_opt("per_page", per_page);
        self.get_json(&format!("/api/tags{}", qs.finish()), Ttl::DAY, "tags")
    }

    pub fn followed_tags(&mut self) -> Result<Vec<FollowedTag>> {
        self.get_json("/api/follows/tags", Ttl::DAY, "follows/tags")
    }

    pub fn comments(&mut self, article_id: i64) -> Result<Vec<Comment>> {
        let path = format!("/api/comments?a_id={article_id}");
        self.get_json(&path, Ttl::MINUTE, "comments")
    }

    /// The bundled analytics panel. Never cached: Forem serves it `Cache-Control: no-store`
    /// because it is a personal view that must reflect activity immediately.
    pub fn analytics_dashboard(
        &mut self,
        start: Option<&str>,
        end: Option<&str>,
        article_id: Option<i64>,
        organization_id: Option<i64>,
    ) -> Result<Dashboard> {
        let mut qs = QueryString::new();
        qs.push_opt_str("start", start);
        qs.push_opt_str("end", end);
        qs.push_opt("article_id", article_id);
        qs.push_opt("organization_id", organization_id);
        let path = format!("/api/analytics/dashboard{}", qs.finish());
        self.get_json(&path, Ttl::NONE, "analytics/dashboard")
    }

    // ---- writes ----------------------------------------------------------------------

    /// Create an article. One write out of a budget of one per second.
    pub fn create_article(&mut self, payload: &ArticlePayload) -> Result<WrittenArticle> {
        self.write_json(Method::Post, "/api/articles", payload, "create article")
    }

    /// Update an article. A partial write: omitted fields are left as they are.
    pub fn update_article(&mut self, id: i64, payload: &ArticlePayload) -> Result<WrittenArticle> {
        self.write_json(
            Method::Put,
            &format!("/api/articles/{id}"),
            payload,
            "update article",
        )
    }

    fn write_json(
        &mut self,
        method: Method,
        path: &str,
        payload: &ArticlePayload,
        context: &str,
    ) -> Result<WrittenArticle> {
        let response = self.send(method, path, Some(payload.to_request_body()))?;
        // Anything cached is now potentially a lie about this account's articles.
        self.cache.clear();
        serde_json::from_str(&response.body).map_err(|source| Error::Decode {
            // The write already happened: dev.to accepted it and answered. Saying so
            // matters, because a caller that reads this as a failure will send it again
            // and end up with two articles.
            context: format!("{context} (the write succeeded; only its reply was unreadable)"),
            source,
        })
    }

    // ---- pipeline --------------------------------------------------------------------

    fn get_json<D: serde::de::DeserializeOwned>(
        &mut self,
        path: &str,
        ttl: Ttl,
        context: &str,
    ) -> Result<D> {
        let body = self.get_raw(path, ttl)?;
        serde_json::from_str(&body).map_err(|source| Error::Decode {
            context: context.to_string(),
            source,
        })
    }

    fn get_raw(&mut self, path: &str, ttl: Ttl) -> Result<String> {
        let key = format!("GET {path}");
        if let Some(hit) = self.cache.get(&key, self.clock.now_millis()) {
            return Ok(hit);
        }
        let response = self.send(Method::Get, path, None)?;
        self.cache
            .put(&key, &response.body, ttl, self.clock.now_millis());
        Ok(response.body)
    }

    /// Pace, send, charge the budget, and map the outcome. A 429 is retried exactly once,
    /// after waiting what the server asked for; a second one is reported rather than
    /// absorbed, because a caller that knows it is throttled can do something useful and a
    /// caller stalled inside a tool call cannot.
    fn send(&mut self, method: Method, path: &str, body: Option<String>) -> Result<HttpResponse> {
        let kind = if method.is_write() {
            RequestKind::Write
        } else {
            RequestKind::Read
        };

        let response = self.send_once(method, path, body.clone(), kind)?;
        if response.status != 429 {
            return self.interpret(response, path);
        }

        // Blocking the limiter is the whole mechanism: `send_once` sleeps on whatever the
        // limiter says. Sleeping here as well would wait twice as long as the server asked.
        let retry_after = parse_retry_after(response.header("retry-after"));
        self.limiter
            .penalize(self.clock.now_millis(), retry_after * 1_000);

        let retried = self.send_once(method, path, body, kind)?;
        if retried.status == 429 {
            return Err(Error::RateLimited {
                retry_after_secs: parse_retry_after(retried.header("retry-after")),
            });
        }
        self.interpret(retried, path)
    }

    fn send_once(
        &mut self,
        method: Method,
        path: &str,
        body: Option<String>,
        kind: RequestKind,
    ) -> Result<HttpResponse> {
        if let Decision::Wait(millis) = self.limiter.check(kind, self.clock.now_millis()) {
            self.clock.sleep_millis(millis);
        }

        let request = HttpRequest {
            method,
            url: format!("{}{}", self.config.base_url, path),
            headers: self.headers(body.is_some()),
            body,
        };

        // Charged before the result is known: a 401 costs the same throttle as a 200.
        self.limiter.record(kind, self.clock.now_millis());
        self.transport.execute(request).map_err(Error::Transport)
    }

    fn headers(&self, has_body: bool) -> Vec<(String, String)> {
        let mut headers = vec![
            ("Accept".to_string(), V1_ACCEPT.to_string()),
            ("User-Agent".to_string(), self.config.user_agent.clone()),
        ];
        if has_body {
            headers.push(("Content-Type".to_string(), "application/json".to_string()));
        }
        if let Some(key) = &self.config.api_key {
            headers.push(("api-key".to_string(), key.clone()));
        }
        headers
    }

    fn interpret(&self, response: HttpResponse, path: &str) -> Result<HttpResponse> {
        // Checked before the status, because a 200 from the deprecated API is the failure
        // this catches: the call appears to work and quietly returns the wrong contract.
        if response.is_v0_response() {
            return Err(Error::VersionDowngrade);
        }

        match response.status {
            200..=299 => Ok(response),
            401 => Err(Error::Unauthorized),
            403 => Err(Error::Forbidden {
                message: message_from_body(&response.body),
            }),
            404 => Err(Error::NotFound {
                what: path.to_string(),
            }),
            422 => Err(Error::Validation {
                message: message_from_body(&response.body),
            }),
            429 => Err(Error::RateLimited {
                retry_after_secs: parse_retry_after(response.header("retry-after")),
            }),
            status => Err(Error::Server {
                status,
                body: message_from_body(&response.body),
            }),
        }
    }
}

// ---- query strings -------------------------------------------------------------------

struct QueryString {
    parts: Vec<String>,
}

impl QueryString {
    fn new() -> Self {
        Self { parts: Vec::new() }
    }

    fn push_str(&mut self, key: &str, value: &str) {
        self.parts.push(format!("{key}={}", encode(value)));
    }

    fn push_opt_str(&mut self, key: &str, value: Option<&str>) {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            self.push_str(key, value);
        }
    }

    fn push_opt<V: std::fmt::Display>(&mut self, key: &str, value: Option<V>) {
        if let Some(value) = value {
            self.parts
                .push(format!("{key}={}", encode(&value.to_string())));
        }
    }

    fn finish(self) -> String {
        if self.parts.is_empty() {
            String::new()
        } else {
            format!("?{}", self.parts.join("&"))
        }
    }
}

/// Percent-encode a query value. Only the unreserved set survives unescaped, which is
/// stricter than necessary and therefore always correct.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A transport that answers from a script and records what it was asked.
    struct MockTransport {
        responses: RefCell<Vec<HttpResponse>>,
        seen: RefCell<Vec<HttpRequest>>,
    }

    impl MockTransport {
        fn new(responses: Vec<HttpResponse>) -> Self {
            Self {
                responses: RefCell::new(responses),
                seen: RefCell::new(Vec::new()),
            }
        }

        fn ok(body: &str) -> HttpResponse {
            Self::status(200, body)
        }

        fn status(status: u16, body: &str) -> HttpResponse {
            HttpResponse {
                status,
                headers: Vec::new(),
                body: body.to_string(),
            }
        }

        fn with_header(mut response: HttpResponse, key: &str, value: &str) -> HttpResponse {
            response.headers.push((key.to_string(), value.to_string()));
            response
        }
    }

    impl Transport for MockTransport {
        fn execute(&self, request: HttpRequest) -> std::result::Result<HttpResponse, String> {
            self.seen.borrow_mut().push(request);
            let mut responses = self.responses.borrow_mut();
            if responses.is_empty() {
                return Err("the mock ran out of scripted responses".to_string());
            }
            Ok(responses.remove(0))
        }
    }

    /// A clock that only moves when the client sleeps, so pacing is observable.
    struct FakeClock {
        now: RefCell<u64>,
        slept: RefCell<Vec<u64>>,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                now: RefCell::new(0),
                slept: RefCell::new(Vec::new()),
            }
        }
    }

    impl Clock for FakeClock {
        fn now_millis(&self) -> u64 {
            *self.now.borrow()
        }

        fn sleep_millis(&self, millis: u64) {
            self.slept.borrow_mut().push(millis);
            *self.now.borrow_mut() += millis;
        }
    }

    fn client(responses: Vec<HttpResponse>) -> DevtoClient<MockTransport, FakeClock> {
        DevtoClient::with_parts(
            Config {
                api_key: Some("secret".to_string()),
                ..Config::default()
            },
            MockTransport::new(responses),
            FakeClock::new(),
        )
    }

    const ME: &str = r#"{"id":1,"username":"copyleftdev"}"#;

    /// The invariant the whole crate exists to hold. Without this header the same URL
    /// serves the deprecated API and nothing complains.
    #[test]
    fn every_request_selects_api_v1() {
        let mut c = client(vec![MockTransport::ok(ME)]);
        c.me().unwrap();

        let seen = c.transport.seen.borrow();
        let accept = seen[0]
            .headers
            .iter()
            .find(|(k, _)| k == "Accept")
            .map(|(_, v)| v.as_str());
        assert_eq!(accept, Some(V1_ACCEPT));
    }

    #[test]
    fn the_client_identifies_itself_and_sends_the_key() {
        let mut c = client(vec![MockTransport::ok(ME)]);
        c.me().unwrap();

        let seen = c.transport.seen.borrow();
        let header = |name: &str| {
            seen[0]
                .headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        assert!(header("User-Agent").unwrap().starts_with("devto-mcp/"));
        assert!(header("User-Agent").unwrap().contains("github.com"));
        assert_eq!(header("api-key").as_deref(), Some("secret"));
    }

    #[test]
    fn an_anonymous_client_sends_no_key() {
        let mut c = DevtoClient::with_parts(
            Config::default(),
            MockTransport::new(vec![MockTransport::ok("[]")]),
            FakeClock::new(),
        );
        assert!(!c.is_authenticated());
        c.tags(None, None).unwrap();
        assert!(
            !c.transport.seen.borrow()[0]
                .headers
                .iter()
                .any(|(k, _)| k == "api-key")
        );
    }

    /// A 200 from the deprecated API is the dangerous case: it looks like success.
    #[test]
    fn a_v0_response_is_an_error_even_when_it_succeeds() {
        let warning = "299 - This endpoint is part of the V0 (beta) API.";
        let mut c = client(vec![MockTransport::with_header(
            MockTransport::ok(ME),
            "warning",
            warning,
        )]);
        assert!(matches!(c.me(), Err(Error::VersionDowngrade)));
    }

    fn error_for(status: u16, body: &str) -> Error {
        client(vec![MockTransport::status(status, body)])
            .me()
            .unwrap_err()
    }

    #[test]
    fn statuses_map_to_the_errors_a_caller_can_act_on() {
        assert!(matches!(error_for(401, ""), Error::Unauthorized));
        assert!(matches!(error_for(404, ""), Error::NotFound { .. }));
        assert!(matches!(
            error_for(500, "boom"),
            Error::Server { status: 500, .. }
        ));

        match error_for(403, r#"{"error":"nope"}"#) {
            Error::Forbidden { message } => assert_eq!(message, "nope"),
            other => panic!("expected forbidden, got {other:?}"),
        }

        // The 422 must carry Forem's own sentence through: it is the only thing that tells
        // a caller which field it got wrong.
        match error_for(422, r#"{"error":"Title can't be blank"}"#) {
            Error::Validation { message } => assert_eq!(message, "Title can't be blank"),
            other => panic!("expected validation, got {other:?}"),
        }
    }

    #[test]
    fn a_malformed_body_is_a_decode_error_naming_the_endpoint() {
        let mut c = client(vec![MockTransport::ok("not json")]);
        match c.me() {
            Err(Error::Decode { context, .. }) => assert_eq!(context, "users/me"),
            other => panic!("expected a decode error, got {other:?}"),
        }
    }

    #[test]
    fn a_transport_failure_is_reported_rather_than_retried() {
        let mut c = client(vec![]);
        assert!(matches!(c.me(), Err(Error::Transport(_))));
        assert_eq!(c.transport.seen.borrow().len(), 1);
    }

    // ---- pacing ----------------------------------------------------------------------

    #[test]
    fn a_fourth_read_in_one_second_waits_for_the_burst_window() {
        let responses = (0..4).map(|_| MockTransport::ok("[]")).collect();
        let mut c = client(responses);
        for _ in 0..4 {
            c.tags(Some(c.budget().reads_this_minute as u32), None)
                .unwrap();
        }
        let slept = c.clock.slept.borrow().clone();
        assert!(
            slept.iter().any(|&ms| ms > 0),
            "the fourth read should have waited, slept={slept:?}"
        );
    }

    /// The server's answer beats our model of it.
    #[test]
    fn a_429_is_retried_once_after_waiting_what_the_server_asked() {
        let throttled =
            MockTransport::with_header(MockTransport::status(429, ""), "retry-after", "7");
        let mut c = client(vec![throttled, MockTransport::ok(ME)]);

        c.me().unwrap();
        assert!(
            c.clock.slept.borrow().contains(&7_000),
            "expected a 7 second wait, got {:?}",
            c.clock.slept.borrow()
        );
        assert_eq!(c.transport.seen.borrow().len(), 2);
    }

    #[test]
    fn a_second_429_is_reported_rather_than_absorbed() {
        let throttled =
            || MockTransport::with_header(MockTransport::status(429, ""), "retry-after", "3");
        let mut c = client(vec![throttled(), throttled()]);

        match c.me() {
            Err(Error::RateLimited { retry_after_secs }) => assert_eq!(retry_after_secs, 3),
            other => panic!("expected a rate-limit error, got {other:?}"),
        }
        assert_eq!(
            c.transport.seen.borrow().len(),
            2,
            "exactly one retry, then give the caller the answer"
        );
    }

    // ---- caching ---------------------------------------------------------------------

    #[test]
    fn a_repeated_read_is_served_from_cache_and_costs_no_budget() {
        let mut c = client(vec![MockTransport::ok("[]")]);
        c.tags(None, None).unwrap();
        let after_first = c.budget().reads_this_minute;

        c.tags(None, None).unwrap();
        assert_eq!(
            c.transport.seen.borrow().len(),
            1,
            "the second read hit the network"
        );
        assert_eq!(c.budget().reads_this_minute, after_first);
        assert_eq!(c.cache_stats(), (1, 1));
    }

    #[test]
    fn different_query_parameters_are_different_cache_entries() {
        let mut c = client(vec![MockTransport::ok("[]"), MockTransport::ok("[]")]);
        c.tags(Some(1), None).unwrap();
        c.tags(Some(2), None).unwrap();
        assert_eq!(c.transport.seen.borrow().len(), 2);
    }

    /// Analytics are `no-store` upstream. Caching them would report stale numbers as live.
    #[test]
    fn analytics_are_never_cached() {
        let mut c = client(vec![MockTransport::ok("{}"), MockTransport::ok("{}")]);
        c.analytics_dashboard(None, None, None, None).unwrap();
        c.analytics_dashboard(None, None, None, None).unwrap();
        assert_eq!(c.transport.seen.borrow().len(), 2);
    }

    // ---- writes ----------------------------------------------------------------------

    const CREATED: &str = r#"{"id":7,"title":"T","published":false,"url":"https://dev.to/x/t"}"#;

    #[test]
    fn a_write_nests_its_fields_under_an_article_key() {
        let mut c = client(vec![MockTransport::ok(CREATED)]);
        c.create_article(&ArticlePayload {
            title: Some("A title".into()),
            body_markdown: Some("Body.".into()),
            published: Some(false),
            tags: Some(vec!["rust".into()]),
            ..Default::default()
        })
        .unwrap();

        let seen = c.transport.seen.borrow();
        assert_eq!(seen[0].method, Method::Post);
        assert_eq!(seen[0].url, "https://dev.to/api/articles");

        let body: serde_json::Value =
            serde_json::from_str(seen[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["article"]["title"], "A title");
        assert_eq!(body["article"]["published"], false);
        assert_eq!(body["article"]["tags"], serde_json::json!(["rust"]));
    }

    /// An update is a partial write. A field left as `None` must not appear at all —
    /// sending it as null would clear the value rather than leave it alone.
    #[test]
    fn absent_fields_are_omitted_rather_than_sent_as_null() {
        let mut c = client(vec![MockTransport::ok(CREATED)]);
        c.update_article(
            7,
            &ArticlePayload {
                title: Some("New title".into()),
                ..Default::default()
            },
        )
        .unwrap();

        let seen = c.transport.seen.borrow();
        assert_eq!(seen[0].method, Method::Put);
        assert_eq!(seen[0].url, "https://dev.to/api/articles/7");

        let raw = seen[0].body.as_deref().unwrap();
        assert!(raw.contains("New title"));
        assert!(!raw.contains("null"), "a partial update sent nulls: {raw}");
        assert!(!raw.contains("body_markdown"));
    }

    #[test]
    fn a_write_declares_a_json_body() {
        let mut c = client(vec![MockTransport::ok(CREATED)]);
        c.create_article(&ArticlePayload::default()).unwrap();
        let seen = c.transport.seen.borrow();
        assert!(
            seen[0]
                .headers
                .iter()
                .any(|(k, v)| k == "Content-Type" && v == "application/json")
        );
    }

    /// A write draws on a different budget from a read, and a much smaller one.
    #[test]
    fn a_write_spends_the_write_budget_not_the_read_budget() {
        let mut c = client(vec![MockTransport::ok(CREATED)]);
        let before = c.budget();
        c.create_article(&ArticlePayload::default()).unwrap();
        let after = c.budget();

        assert_eq!(after.writes_this_second, before.writes_this_second - 1);
        assert_eq!(after.reads_this_minute, before.reads_this_minute);
    }

    /// Anything cached before a write is now potentially a lie about this account.
    #[test]
    fn a_write_invalidates_everything_that_was_cached() {
        let mut c = client(vec![
            MockTransport::ok("[]"),
            MockTransport::ok(CREATED),
            MockTransport::ok("[]"),
        ]);
        c.tags(None, None).unwrap();
        assert_eq!(c.transport.seen.borrow().len(), 1);

        c.create_article(&ArticlePayload::default()).unwrap();

        c.tags(None, None).unwrap();
        assert_eq!(
            c.transport.seen.borrow().len(),
            3,
            "the cached read survived a write"
        );
    }

    #[test]
    fn a_rejected_write_surfaces_forems_own_sentence() {
        let mut c = client(vec![MockTransport::status(
            422,
            r#"{"error":"Tag is invalid","status":422}"#,
        )]);
        match c.create_article(&ArticlePayload::default()) {
            Err(Error::Validation { message }) => assert_eq!(message, "Tag is invalid"),
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    /// A decode failure after a write is the dangerous case: the article exists, and a
    /// caller who reads this as "it failed" will create a second one.
    #[test]
    fn a_write_whose_reply_is_unreadable_says_the_write_still_happened() {
        let mut c = client(vec![MockTransport::ok("not json at all")]);
        match c.create_article(&ArticlePayload::default()) {
            Err(Error::Decode { context, .. }) => {
                assert!(context.contains("the write succeeded"), "{context}");
            }
            other => panic!("expected a decode error, got {other:?}"),
        }
    }

    #[test]
    fn a_write_response_parses_even_when_it_is_sparse() {
        let mut c = client(vec![MockTransport::ok(r#"{"id":7}"#)]);
        let written = c.create_article(&ArticlePayload::default()).unwrap();
        assert_eq!(written.id, 7);
        assert!(!written.published);
        assert!(written.tag_list.is_empty());
    }

    // ---- urls ------------------------------------------------------------------------

    #[test]
    fn query_values_are_percent_encoded() {
        let mut c = client(vec![MockTransport::ok("[]")]);
        c.semantic_search("rust ownership & lifetimes", None, None, None)
            .unwrap();
        let url = c.transport.seen.borrow()[0].url.clone();
        assert!(
            url.contains("q=rust%20ownership%20%26%20lifetimes"),
            "{url}"
        );
        assert!(url.starts_with("https://dev.to/api/articles/semantic_search?"));
    }

    #[test]
    fn absent_query_parameters_are_omitted_entirely() {
        let mut c = client(vec![MockTransport::ok("[]")]);
        c.articles(ArticleQuery::default()).unwrap();
        assert_eq!(
            c.transport.seen.borrow()[0].url,
            "https://dev.to/api/articles"
        );
    }

    #[test]
    fn the_my_articles_status_selects_the_path() {
        for (status, expected) in [
            (MyArticleStatus::Published, "published"),
            (MyArticleStatus::Unpublished, "unpublished"),
            (MyArticleStatus::All, "all"),
        ] {
            let mut c = client(vec![MockTransport::ok("[]")]);
            c.my_articles(status, None, None).unwrap();
            assert!(
                c.transport.seen.borrow()[0]
                    .url
                    .ends_with(&format!("/api/articles/me/{expected}")),
                "{status:?}"
            );
        }
    }

    #[test]
    fn a_path_lookup_escapes_both_segments() {
        let mut c = client(vec![MockTransport::ok("{\"id\":1,\"title\":\"t\"}")]);
        c.article_by_path("a user", "a/slug").unwrap();
        assert_eq!(
            c.transport.seen.borrow()[0].url,
            "https://dev.to/api/articles/a%20user/a%2Fslug"
        );
    }

    #[test]
    fn the_base_url_is_configurable_for_other_forem_instances() {
        let mut c = DevtoClient::with_parts(
            Config {
                base_url: "https://forem.example".to_string(),
                ..Config::default()
            },
            MockTransport::new(vec![MockTransport::ok("[]")]),
            FakeClock::new(),
        );
        c.tags(None, None).unwrap();
        assert!(
            c.transport.seen.borrow()[0]
                .url
                .starts_with("https://forem.example/")
        );
    }

    #[test]
    fn configuration_is_reported_back_accurately() {
        let c = client(vec![]);
        assert!(c.is_authenticated());
        assert_eq!(c.base_url(), "https://dev.to");

        let anonymous: DevtoClient<MockTransport, FakeClock> = DevtoClient::with_parts(
            Config {
                base_url: "https://forem.example".to_string(),
                ..Config::default()
            },
            MockTransport::new(vec![]),
            FakeClock::new(),
        );
        assert!(!anonymous.is_authenticated());
        assert_eq!(anonymous.base_url(), "https://forem.example");
    }

    #[test]
    fn cache_statistics_start_empty_and_count_each_outcome() {
        let mut c = client(vec![MockTransport::ok("[]")]);
        assert_eq!(c.cache_stats(), (0, 0));
        c.tags(None, None).unwrap();
        assert_eq!(c.cache_stats(), (0, 1), "the first read is a miss");
        c.tags(None, None).unwrap();
        assert_eq!(c.cache_stats(), (1, 1), "the second is a hit");
    }

    #[test]
    fn the_remaining_read_endpoints_reach_the_paths_they_name() {
        let mut c = client(vec![MockTransport::ok("[]")]);
        c.search_articles("rust", Some(2), Some(5)).unwrap();
        assert_eq!(
            c.transport.seen.borrow()[0].url,
            "https://dev.to/api/articles/search?q=rust&page=2&per_page=5"
        );

        let mut c = client(vec![MockTransport::ok("[]")]);
        c.followed_tags().unwrap();
        assert_eq!(
            c.transport.seen.borrow()[0].url,
            "https://dev.to/api/follows/tags"
        );

        let mut c = client(vec![MockTransport::ok("[]")]);
        c.comments(12345).unwrap();
        assert_eq!(
            c.transport.seen.borrow()[0].url,
            "https://dev.to/api/comments?a_id=12345"
        );

        let mut c = client(vec![MockTransport::ok(r#"{"id":9,"title":"t"}"#)]);
        c.article(9).unwrap();
        assert_eq!(
            c.transport.seen.borrow()[0].url,
            "https://dev.to/api/articles/9"
        );
    }

    #[test]
    fn the_endpoints_decode_what_forem_sends() {
        let mut c = client(vec![MockTransport::ok(
            r#"[{"id":1,"name":"rust","points":3.5}]"#,
        )]);
        let tags = c.followed_tags().unwrap();
        assert_eq!(tags[0].name, "rust");
        assert_eq!(tags[0].points, 3.5);

        let mut c = client(vec![MockTransport::ok(
            r#"[{"id_code":"abc","body_html":"<p>hi</p>","children":[]}]"#,
        )]);
        let comments = c.comments(1).unwrap();
        assert_eq!(comments[0].id_code, "abc");

        let mut c = client(vec![MockTransport::ok(r#"[{"id":7,"title":"Found"}]"#)]);
        let found = c.search_articles("q", None, None).unwrap();
        assert_eq!(found[0].title, "Found");
    }

    /// An empty string is an absent value, not a value of "". Sending `?tag=` would filter
    /// the feed to articles tagged with nothing at all.
    #[test]
    fn an_empty_string_parameter_is_omitted_not_sent_blank() {
        let mut c = client(vec![MockTransport::ok("[]")]);
        c.articles(ArticleQuery {
            tag: Some(""),
            username: Some("copyleftdev"),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            c.transport.seen.borrow()[0].url,
            "https://dev.to/api/articles?username=copyleftdev"
        );
    }

    /// The retry waits exactly what the server asked — once. The limiter's own block is
    /// the mechanism, so an extra sleep here would double every throttle wait.
    #[test]
    fn the_retry_wait_is_the_servers_number_and_is_not_doubled() {
        let throttled =
            MockTransport::with_header(MockTransport::status(429, ""), "retry-after", "7");
        let mut c = client(vec![throttled, MockTransport::ok(ME)]);
        c.me().unwrap();

        let slept: u64 = c.clock.slept.borrow().iter().sum();
        assert_eq!(
            slept,
            7_000,
            "expected exactly one 7 second wait, got {:?}",
            c.clock.slept.borrow()
        );
    }

    #[hegel::test]
    fn encoding_leaves_only_unreserved_characters(tc: hegel::TestCase) {
        let raw = tc.draw(hegel::generators::text().max_size(60));
        let encoded = encode(&raw);
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.~%".contains(c)),
            "{raw:?} encoded to {encoded:?}"
        );
        if raw.chars().all(|c| c.is_ascii_alphanumeric()) {
            assert_eq!(encoded, raw);
        }
    }
}
