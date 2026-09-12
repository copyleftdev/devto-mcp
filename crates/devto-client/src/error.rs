//! Error mapping.
//!
//! The split that matters for an MCP surface: some of these a model can fix by calling
//! again with different arguments (`Validation`, `RateLimited`), and some it cannot
//! (`Unauthorized`, `VersionDowngrade`). The tool layer hands the first kind back to the
//! model and the second kind to the human.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// No key, a rejected key, or a suspended account. Forem does not distinguish.
    #[error(
        "dev.to rejected the API key (401). Check DEVTO_API_KEY at https://dev.to/settings/extensions"
    )]
    Unauthorized,

    /// The endpoint exists but this key may not use it. On dev.to this is what a normal
    /// key gets from the admin-only surface, reactions included.
    #[error("dev.to refused this action for your account (403): {message}")]
    Forbidden { message: String },

    #[error("not found (404): {what}")]
    NotFound { what: String },

    /// A 422 carrying Forem's own sentence. `devto-core` exists to make these rare.
    #[error("dev.to rejected the payload (422): {message}")]
    Validation { message: String },

    /// Rack::Attack. `retry_after` comes from the response header when present.
    #[error("rate limited by dev.to (429); retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("dev.to returned {status}: {body}")]
    Server { status: u16, body: String },

    /// Forem stamps `Warning: 299` on every V0 response. Seeing it means our `Accept`
    /// header did not survive the trip, and we are silently talking to the deprecated API.
    /// That is a client defect, not a server condition, so it is never retried.
    #[error(
        "request was served by the deprecated V0 API — the \
         `Accept: application/vnd.forem.api-v1+json` header did not reach dev.to"
    )]
    VersionDowngrade,

    #[error("transport error: {0}")]
    Transport(String),

    #[error("could not decode the {context} response: {source}")]
    Decode {
        context: String,
        #[source]
        source: serde_json::Error,
    },
}

impl Error {
    /// Whether calling again unchanged could plausibly succeed. A 429 will; a 401 will not.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Server { .. })
    }

    /// Whether the caller can fix this by changing what it sends.
    pub fn is_callers_fault(&self) -> bool {
        matches!(self, Self::Validation { .. } | Self::NotFound { .. })
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Forem's JSON error envelope is `{"error": "...", "status": 422}`. Some endpoints send a
/// bare string or a `message` array instead, so fall back to the raw body rather than
/// losing what the server said.
pub(crate) fn message_from_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "(no response body)".to_string();
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => value
            .get("error")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                value
                    .get("message")
                    .map(|m| match m {
                        serde_json::Value::Array(items) => items
                            .iter()
                            .filter_map(|i| i.as_str())
                            .collect::<Vec<_>>()
                            .join("; "),
                        other => other.to_string(),
                    })
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| trimmed.to_string()),
        Err(_) => trimmed.to_string(),
    }
}

/// `Retry-After` is seconds in Forem's case, but the header also permits an HTTP date.
/// An unparseable value falls back to one second, which is the shortest throttle window.
pub(crate) fn parse_retry_after(value: Option<&str>) -> u64 {
    value
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(1)
        .clamp(1, 3_600)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_forem_error_envelope_is_unwrapped() {
        assert_eq!(
            message_from_body(r#"{"error":"Title can't be blank","status":422}"#),
            "Title can't be blank"
        );
    }

    #[test]
    fn a_message_array_is_joined() {
        assert_eq!(
            message_from_body(r#"{"message":["Title can't be blank","Tag is invalid"]}"#),
            "Title can't be blank; Tag is invalid"
        );
    }

    #[test]
    fn an_unrecognised_body_is_passed_through_rather_than_lost() {
        assert_eq!(message_from_body("upstream exploded"), "upstream exploded");
        assert_eq!(message_from_body(r#"{"other":1}"#), r#"{"other":1}"#);
        assert_eq!(message_from_body("   "), "(no response body)");
    }

    #[test]
    fn an_empty_message_falls_back_to_the_body() {
        assert_eq!(message_from_body(r#"{"message":[]}"#), r#"{"message":[]}"#);
    }

    #[test]
    fn retry_after_is_seconds_and_is_bounded() {
        assert_eq!(parse_retry_after(Some("30")), 30);
        assert_eq!(parse_retry_after(Some("  5 ")), 5);
        assert_eq!(parse_retry_after(None), 1);
        assert_eq!(parse_retry_after(Some("0")), 1);
        assert_eq!(parse_retry_after(Some("not a number")), 1);
        assert_eq!(parse_retry_after(Some("99999")), 3_600);
    }

    #[test]
    fn only_transient_conditions_are_retryable() {
        assert!(
            Error::RateLimited {
                retry_after_secs: 1
            }
            .is_retryable()
        );
        assert!(
            Error::Server {
                status: 503,
                body: String::new()
            }
            .is_retryable()
        );
        assert!(!Error::Unauthorized.is_retryable());
        assert!(!Error::VersionDowngrade.is_retryable());
        assert!(
            !Error::Validation {
                message: String::new()
            }
            .is_retryable()
        );
    }

    #[test]
    fn payload_problems_are_the_callers_to_fix() {
        assert!(
            Error::Validation {
                message: String::new()
            }
            .is_callers_fault()
        );
        assert!(
            Error::NotFound {
                what: String::new()
            }
            .is_callers_fault()
        );
        assert!(!Error::Unauthorized.is_callers_fault());
        assert!(
            !Error::RateLimited {
                retry_after_secs: 1
            }
            .is_callers_fault()
        );
    }

    #[hegel::test]
    fn extracting_a_message_never_panics_and_never_returns_nothing(tc: hegel::TestCase) {
        let body = tc.draw(hegel::generators::text().max_size(200));
        assert!(!message_from_body(&body).is_empty());
    }

    #[hegel::test]
    fn retry_after_is_always_a_usable_delay(tc: hegel::TestCase) {
        let raw = tc.draw(hegel::generators::text().max_size(20));
        let secs = parse_retry_after(Some(&raw));
        assert!((1..=3_600).contains(&secs));
    }
}
