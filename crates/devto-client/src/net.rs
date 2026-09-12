//! The network adapter.
//!
//! Deliberately the thinnest file in the crate, and deliberately excluded from the
//! mutation gate: nothing here can be killed by a unit test, because every line of it is
//! either a ureq call or a wall-clock read. Everything that can hold a bug — pacing,
//! caching, error mapping, URL building — lives on the other side of the
//! [`Transport`](crate::transport::Transport) and [`Clock`](crate::transport::Clock)
//! traits, where a test can reach it without spending dev.to's budget.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::transport::{Clock, HttpRequest, HttpResponse, Method, Transport};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn sleep_millis(&self, millis: u64) {
        if millis > 0 {
            std::thread::sleep(std::time::Duration::from_millis(millis));
        }
    }
}

/// The real transport.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl UreqTransport {
    pub fn new() -> Self {
        // Status codes are mapped by us, not raised as transport errors: a 422 carries the
        // message the caller needs and must not be flattened into "request failed".
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.new_agent(),
        }
    }
}

impl Transport for UreqTransport {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, String> {
        let mut response = match request.method {
            Method::Get => {
                let mut builder = self.agent.get(&request.url);
                for (key, value) in &request.headers {
                    builder = builder.header(key, value);
                }
                builder.call()
            }
            Method::Post | Method::Put => {
                let mut builder = if request.method == Method::Post {
                    self.agent.post(&request.url)
                } else {
                    self.agent.put(&request.url)
                };
                for (key, value) in &request.headers {
                    builder = builder.header(key, value);
                }
                match &request.body {
                    Some(body) => builder.send(body.as_str()),
                    None => builder.send_empty(),
                }
            }
        }
        .map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    v.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}
