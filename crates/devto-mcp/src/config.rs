//! What this server is allowed to do, read from the environment.
//!
//! dev.to issues one unscoped API key: it carries the account's whole identity and every
//! privilege it holds, and there is no read-only variant. So the only place a
//! least-privilege boundary can exist is here. Capabilities are off unless the human
//! turned them on deliberately.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    /// Whether the server may set `published: true`. Off by default.
    pub publish: bool,
    /// Whether the server may send `ai_disclosure_level: no_ai`.
    ///
    /// Off by default, and the reasoning is worth stating: a model calling these tools
    /// cannot honestly certify that an article was written without meaningful AI
    /// assistance, and dev.to's llms.txt is explicit that a human merely reviewing
    /// autonomous output does not make it human-authored. A human who genuinely drafted
    /// the piece and is using this only as transport turns it on in one line.
    pub claim_no_ai: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub api_key: Option<String>,
    pub capabilities: Capabilities,
}

impl Config {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Split from `from_env` so the parsing rules are testable without touching the
    /// process environment, which tests cannot safely share.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            base_url: lookup("DEVTO_BASE_URL")
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| devto_client::DEFAULT_BASE_URL.to_string()),
            api_key: lookup("DEVTO_API_KEY").filter(|v| !v.trim().is_empty()),
            capabilities: Capabilities {
                publish: is_enabled(lookup("DEVTO_PUBLISH").as_deref()),
                claim_no_ai: is_enabled(lookup("DEVTO_ALLOW_NO_AI_CLAIM").as_deref()),
            },
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.api_key.is_some()
    }
}

/// A capability is on only for an unambiguous yes. Anything else — unset, empty, "0",
/// "maybe" — leaves it off, because the failure mode of guessing wrong is a published
/// article nobody approved.
fn is_enabled(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on" | "enabled")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from(pairs: &[(&str, &str)]) -> Config {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(move |key| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()))
    }

    /// The defaults are the whole safety argument: an operator who sets only a key gets a
    /// server that can read and draft and nothing else.
    #[test]
    fn nothing_is_granted_by_default() {
        let config = from(&[]);
        assert!(!config.is_authenticated());
        assert!(!config.capabilities.publish);
        assert!(!config.capabilities.claim_no_ai);
        assert_eq!(config.base_url, "https://dev.to");
    }

    #[test]
    fn a_key_alone_grants_reading_and_drafting_but_not_publishing() {
        let config = from(&[("DEVTO_API_KEY", "secret")]);
        assert!(config.is_authenticated());
        assert!(!config.capabilities.publish);
        assert!(!config.capabilities.claim_no_ai);
    }

    #[test]
    fn capabilities_turn_on_for_an_unambiguous_yes() {
        for value in ["1", "true", "TRUE", "yes", "on", "enabled", "  true  "] {
            let config = from(&[("DEVTO_PUBLISH", value)]);
            assert!(config.capabilities.publish, "{value:?} should enable");
        }
    }

    #[test]
    fn anything_ambiguous_leaves_a_capability_off() {
        for value in ["", "  ", "0", "false", "no", "off", "maybe", "please"] {
            let config = from(&[("DEVTO_PUBLISH", value)]);
            assert!(!config.capabilities.publish, "{value:?} should not enable");
        }
    }

    #[test]
    fn the_two_capabilities_are_granted_independently() {
        let publish_only = from(&[("DEVTO_PUBLISH", "true")]);
        assert!(publish_only.capabilities.publish);
        assert!(!publish_only.capabilities.claim_no_ai);

        let claim_only = from(&[("DEVTO_ALLOW_NO_AI_CLAIM", "true")]);
        assert!(!claim_only.capabilities.publish);
        assert!(claim_only.capabilities.claim_no_ai);
    }

    #[test]
    fn a_blank_key_is_no_key() {
        assert!(!from(&[("DEVTO_API_KEY", "   ")]).is_authenticated());
        assert!(!from(&[("DEVTO_API_KEY", "")]).is_authenticated());
    }

    /// Forem is one codebase serving many instances; only the host differs.
    #[test]
    fn the_base_url_can_point_at_another_forem() {
        assert_eq!(
            from(&[("DEVTO_BASE_URL", "https://forem.example")]).base_url,
            "https://forem.example"
        );
        assert_eq!(
            from(&[("DEVTO_BASE_URL", "  ")]).base_url,
            "https://dev.to",
            "a blank override falls back rather than producing an empty host"
        );
    }
}
