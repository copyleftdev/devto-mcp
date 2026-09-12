//! The HTTP seam.
//!
//! Request and response shapes plus the two traits the client is generic over. The real
//! implementations live in [`crate::net`]; keeping them apart is what lets the pacing,
//! caching and error mapping be tested against scripted responses rather than against
//! dev.to, whose budget is 30 reads a minute.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
        }
    }

    /// Forem throttles reads and writes separately, keyed on the method.
    pub fn is_write(self) -> bool {
        !matches!(self, Self::Get)
    }
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Forem stamps `Warning: 299 - This endpoint is part of the V0 (beta) API...` on every
    /// V0 response. If we see it, our version header was lost in transit.
    pub fn is_v0_response(&self) -> bool {
        self.header("warning")
            .is_some_and(|w| w.contains("299") && w.contains("V0"))
    }
}

/// Anything that can perform an HTTP request.
pub trait Transport {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, String>;
}

/// A monotonic-enough millisecond clock. Separate from `Transport` so tests can advance
/// time without performing requests.
pub trait Clock {
    fn now_millis(&self) -> u64;
    fn sleep_millis(&self, millis: u64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_get_is_a_read() {
        assert!(!Method::Get.is_write());
        assert!(Method::Post.is_write());
        assert!(Method::Put.is_write());
        assert_eq!(Method::Get.as_str(), "GET");
        assert_eq!(Method::Post.as_str(), "POST");
        assert_eq!(Method::Put.as_str(), "PUT");
    }

    fn response_with(headers: &[(&str, &str)]) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: String::new(),
        }
    }

    #[test]
    fn header_lookup_ignores_case() {
        let response = response_with(&[("Content-Type", "application/json")]);
        assert_eq!(response.header("content-type"), Some("application/json"));
        assert_eq!(response.header("CONTENT-TYPE"), Some("application/json"));
        assert_eq!(response.header("missing"), None);
    }

    /// This is the client's own smoke alarm: if the version header stops arriving, dev.to
    /// serves the deprecated API and says so in a header nobody reads.
    #[test]
    fn the_v0_deprecation_warning_is_recognised() {
        let warning = "299 - This endpoint is part of the V0 (beta) API. To start using the \
                       V1 endpoints add the `Accept` header";
        assert!(response_with(&[("warning", warning)]).is_v0_response());
        assert!(response_with(&[("Warning", warning)]).is_v0_response());

        assert!(!response_with(&[]).is_v0_response());
        assert!(!response_with(&[("warning", "299 - something else entirely")]).is_v0_response());
        assert!(!response_with(&[("warning", "110 - Response is stale V0")]).is_v0_response());
    }
}
