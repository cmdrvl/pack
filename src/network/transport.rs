use crate::refusal::{RefusalCode, RefusalEnvelope};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;

pub const DATA_FABRIC_TIMEOUT_SECS_ENV: &str = "PACK_DATA_FABRIC_TIMEOUT_SECS";
pub const DATA_FABRIC_RETRIES_ENV: &str = "PACK_DATA_FABRIC_RETRIES";
pub const DATA_FABRIC_RETRY_BACKOFF_MS_ENV: &str = "PACK_DATA_FABRIC_RETRY_BACKOFF_MS";

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_RETRIES: u32 = 2;
const DEFAULT_RETRY_BACKOFF_MS: u64 = 100;
const MAX_TIMEOUT_SECS: u64 = 3_600;
const MAX_RETRIES: u32 = 10;
const MAX_RETRY_BACKOFF_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMethod {
    Get,
    Post,
    Put,
}

impl TransportMethod {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
        }
    }

    fn is_idempotent(self) -> bool {
        matches!(self, Self::Get | Self::Put)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportRequest {
    pub method: TransportMethod,
    pub path: String,
    pub body: Option<Value>,
}

impl TransportRequest {
    pub fn get(path: impl Into<String>) -> Self {
        Self {
            method: TransportMethod::Get,
            path: path.into(),
            body: None,
        }
    }

    pub fn post(path: impl Into<String>, body: Value) -> Self {
        Self {
            method: TransportMethod::Post,
            path: path.into(),
            body: Some(body),
        }
    }

    pub fn put(path: impl Into<String>, body: Value) -> Self {
        Self {
            method: TransportMethod::Put,
            path: path.into(),
            body: Some(body),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportResponse {
    pub status: u16,
    pub body: Option<Value>,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Network {
        message: String,
        attempts: u32,
    },
    Server {
        status: u16,
        body: Option<Value>,
        attempts: u32,
    },
    Decode {
        message: String,
        attempts: u32,
    },
}

impl TransportError {
    fn with_attempts(self, attempts: u32) -> Self {
        match self {
            Self::Network { message, .. } => Self::Network { message, attempts },
            Self::Server { status, body, .. } => Self::Server {
                status,
                body,
                attempts,
            },
            Self::Decode { message, .. } => Self::Decode { message, attempts },
        }
    }

    fn attempts(&self) -> u32 {
        match self {
            Self::Network { attempts, .. }
            | Self::Server { attempts, .. }
            | Self::Decode { attempts, .. } => *attempts,
        }
    }

    fn is_retryable(&self) -> bool {
        match self {
            Self::Network { .. } => true,
            Self::Server { status, .. } => *status == 408 || *status == 429 || *status >= 500,
            Self::Decode { .. } => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportPolicy {
    pub timeout: Duration,
    pub max_retries: u32,
    pub retry_backoff: Duration,
}

impl Default for TransportPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            max_retries: DEFAULT_RETRIES,
            retry_backoff: Duration::from_millis(DEFAULT_RETRY_BACKOFF_MS),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DataFabricTransport {
    base_url: String,
    policy: TransportPolicy,
}

impl DataFabricTransport {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            policy: TransportPolicy::default(),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.policy.timeout = timeout;
        self
    }

    pub fn with_policy(mut self, policy: TransportPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn send(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        let mut attempts = 0;

        loop {
            attempts += 1;
            match self.send_once(request) {
                Ok((status, body)) => {
                    return Ok(TransportResponse {
                        status,
                        body,
                        attempts,
                    });
                }
                Err(error) => {
                    let error = error.with_attempts(attempts);
                    if self.should_retry(request, &error) {
                        sleep_before_retry(self.policy.retry_backoff, attempts);
                        continue;
                    }
                    return Err(error);
                }
            }
        }
    }

    fn send_once(
        &self,
        request: &TransportRequest,
    ) -> Result<(u16, Option<Value>), TransportError> {
        let url = build_url(&self.base_url, &request.path);
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(self.policy.timeout)
            .timeout_read(self.policy.timeout)
            .timeout_write(self.policy.timeout)
            .build();

        let result = match request.method {
            TransportMethod::Get => agent.get(&url).set("Accept", "application/json").call(),
            TransportMethod::Post => agent
                .post(&url)
                .set("Accept", "application/json")
                .set("Content-Type", "application/json")
                .send_json(request.body.clone().unwrap_or(Value::Null)),
            TransportMethod::Put => agent
                .put(&url)
                .set("Accept", "application/json")
                .set("Content-Type", "application/json")
                .send_json(request.body.clone().unwrap_or(Value::Null)),
        };

        match result {
            Ok(response) => {
                let status = response.status();
                let body =
                    decode_success_body(response).map_err(|message| TransportError::Decode {
                        message,
                        attempts: 0,
                    })?;
                Ok((status, body))
            }
            Err(ureq::Error::Status(status, response)) => Err(TransportError::Server {
                status,
                body: decode_error_body(response)
                    .unwrap_or_else(|message| Some(Value::String(message))),
                attempts: 0,
            }),
            Err(ureq::Error::Transport(error)) => Err(TransportError::Network {
                message: error.to_string(),
                attempts: 0,
            }),
        }
    }

    fn should_retry(&self, request: &TransportRequest, error: &TransportError) -> bool {
        request.method.is_idempotent()
            && error.is_retryable()
            && error.attempts() <= self.policy.max_retries
    }

    pub fn send_json<T: DeserializeOwned>(
        &self,
        request: &TransportRequest,
    ) -> Result<T, TransportError> {
        let response = self.send(request)?;
        let body = response.body.ok_or_else(|| TransportError::Decode {
            message: format!(
                "Expected JSON response body for {} {}",
                request.method.as_str(),
                request.path
            ),
            attempts: response.attempts,
        })?;
        serde_json::from_value(body).map_err(|error| TransportError::Decode {
            message: format!("Failed to decode transport response: {error}"),
            attempts: response.attempts,
        })
    }
}

pub fn refusal_for_transport(action: &str, error: &TransportError) -> RefusalEnvelope {
    let (message, detail) = match error {
        TransportError::Network { message, attempts } => (
            format!("pack {action} transport failure: {message}"),
            serde_json::json!({
                "action": action,
                "kind": "network",
                "message": message,
                "attempts": attempts,
            }),
        ),
        TransportError::Server {
            status,
            body,
            attempts,
        } => (
            format!("pack {action} server failure: HTTP {status}"),
            serde_json::json!({
                "action": action,
                "kind": "server",
                "status": status,
                "body": body,
                "attempts": attempts,
            }),
        ),
        TransportError::Decode { message, attempts } => (
            format!("pack {action} transport decode failure: {message}"),
            serde_json::json!({
                "action": action,
                "kind": "decode",
                "message": message,
                "attempts": attempts,
            }),
        ),
    };
    RefusalEnvelope::new(RefusalCode::Io, Some(message), Some(detail))
}

pub fn deferred_network_refusal(command: &str) -> RefusalEnvelope {
    RefusalEnvelope::new(
        RefusalCode::Io,
        Some(format!("pack {command}: deferred in v0.1")),
        Some(serde_json::json!({
            "command": command,
            "status": "deferred",
        })),
    )
}

fn build_url(base_url: &str, path: &str) -> String {
    if path.starts_with('/') {
        format!("{base_url}{path}")
    } else {
        format!("{base_url}/{path}")
    }
}

pub fn transport_policy_from_env<F>(mut get_env: F) -> Result<TransportPolicy, Box<RefusalEnvelope>>
where
    F: FnMut(&str) -> Option<String>,
{
    let timeout_secs = parse_optional_bounded_u64(
        get_env(DATA_FABRIC_TIMEOUT_SECS_ENV),
        DATA_FABRIC_TIMEOUT_SECS_ENV,
        DEFAULT_TIMEOUT_SECS,
        1,
        MAX_TIMEOUT_SECS,
        "seconds between 1 and 3600",
    )?;
    let max_retries = parse_optional_bounded_u32(
        get_env(DATA_FABRIC_RETRIES_ENV),
        DATA_FABRIC_RETRIES_ENV,
        DEFAULT_RETRIES,
        0,
        MAX_RETRIES,
        "integer between 0 and 10",
    )?;
    let retry_backoff_ms = parse_optional_bounded_u64(
        get_env(DATA_FABRIC_RETRY_BACKOFF_MS_ENV),
        DATA_FABRIC_RETRY_BACKOFF_MS_ENV,
        DEFAULT_RETRY_BACKOFF_MS,
        0,
        MAX_RETRY_BACKOFF_MS,
        "milliseconds between 0 and 60000",
    )?;

    Ok(TransportPolicy {
        timeout: Duration::from_secs(timeout_secs),
        max_retries,
        retry_backoff: Duration::from_millis(retry_backoff_ms),
    })
}

fn parse_optional_bounded_u64(
    value: Option<String>,
    env: &str,
    default: u64,
    min: u64,
    max: u64,
    expected: &str,
) -> Result<u64, Box<RefusalEnvelope>> {
    let Some(raw) = value else {
        return Ok(default);
    };
    parse_bounded(&raw, env, min, max, expected)
}

fn parse_optional_bounded_u32(
    value: Option<String>,
    env: &str,
    default: u32,
    min: u32,
    max: u32,
    expected: &str,
) -> Result<u32, Box<RefusalEnvelope>> {
    let Some(raw) = value else {
        return Ok(default);
    };
    parse_bounded(&raw, env, min, max, expected)
}

fn parse_bounded<T>(
    raw: &str,
    env: &str,
    min: T,
    max: T,
    expected: &str,
) -> Result<T, Box<RefusalEnvelope>>
where
    T: std::str::FromStr + PartialOrd + Copy + serde::Serialize,
{
    let trimmed = raw.trim();
    let value = trimmed.parse::<T>().map_err(|_| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Invalid {env}: expected {expected}")),
            Some(serde_json::json!({
                "env": env,
                "value": raw,
                "expected": expected,
            })),
        ))
    })?;

    if value < min || value > max {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Invalid {env}: expected {expected}")),
            Some(serde_json::json!({
                "env": env,
                "value": raw,
                "expected": expected,
                "min": min,
                "max": max,
            })),
        )));
    }

    Ok(value)
}

fn decode_success_body(response: ureq::Response) -> Result<Option<Value>, String> {
    let text = response.into_string().map_err(|error| error.to_string())?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text).map(Some).map_err(|error| {
        format!("Expected JSON response body, but response was not valid JSON: {error}")
    })
}

fn decode_error_body(response: ureq::Response) -> Result<Option<Value>, String> {
    let text = response.into_string().map_err(|error| error.to_string())?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text)
        .map(Some)
        .or(Ok(Some(Value::String(text))))
}

fn sleep_before_retry(base_delay: Duration, completed_attempts: u32) {
    if base_delay.is_zero() {
        return;
    }

    let delay_ms = base_delay
        .as_millis()
        .saturating_mul(u128::from(completed_attempts))
        .min(u128::from(u64::MAX)) as u64;
    std::thread::sleep(Duration::from_millis(delay_ms));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        thread::{self, JoinHandle},
    };
    use tiny_http::{Header, Response, Server, StatusCode};

    type RecordedRequest = (String, String);
    type MockResponse = (u16, &'static str, String);

    struct MockServer {
        base_url: String,
        requests: mpsc::Receiver<RecordedRequest>,
        handle: JoinHandle<()>,
    }

    impl MockServer {
        fn finish(self) -> Vec<RecordedRequest> {
            self.handle.join().unwrap();
            self.requests.try_iter().collect()
        }
    }

    fn spawn_server(responses: Vec<MockResponse>) -> MockServer {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr());
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            for (status, path, body) in responses {
                let mut request = server.recv().unwrap();
                let mut request_body = String::new();
                request
                    .as_reader()
                    .read_to_string(&mut request_body)
                    .unwrap();
                tx.send((request.url().to_string(), request_body)).unwrap();
                let response = Response::from_string(body)
                    .with_status_code(StatusCode(status))
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
                assert_eq!(request.url(), path);
                request.respond(response).unwrap();
            }
        });

        MockServer {
            base_url,
            requests: rx,
            handle,
        }
    }

    #[test]
    fn send_json_round_trips_post_request() {
        let server = spawn_server(vec![(
            200,
            "/packs/sha256:abc",
            serde_json::json!({ "accepted": true }).to_string(),
        )]);
        let transport =
            DataFabricTransport::new(&server.base_url).with_timeout(Duration::from_secs(2));
        let response: Value = transport
            .send_json(&TransportRequest::post(
                "/packs/sha256:abc",
                serde_json::json!({ "pack_id": "sha256:abc" }),
            ))
            .unwrap();

        assert_eq!(response["accepted"], true);
        let requests = server.finish();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].1.contains("\"pack_id\":\"sha256:abc\""));
    }

    #[test]
    fn retryable_server_failures_are_retried_for_idempotent_requests() {
        let server = spawn_server(vec![
            (
                503,
                "/packs/sha256:abc",
                serde_json::json!({ "error": "try later" }).to_string(),
            ),
            (
                502,
                "/packs/sha256:abc",
                serde_json::json!({ "error": "still later" }).to_string(),
            ),
            (
                200,
                "/packs/sha256:abc",
                serde_json::json!({ "accepted": true }).to_string(),
            ),
        ]);
        let transport = DataFabricTransport::new(&server.base_url).with_policy(TransportPolicy {
            timeout: Duration::from_secs(2),
            max_retries: 2,
            retry_backoff: Duration::from_millis(0),
        });

        let response: Value = transport
            .send_json(&TransportRequest::get("/packs/sha256:abc"))
            .unwrap();

        assert_eq!(response["accepted"], true);
        assert_eq!(server.finish().len(), 3);
    }

    #[test]
    fn non_retryable_server_failure_is_not_retried() {
        let server = spawn_server(vec![(
            404,
            "/packs/sha256:missing",
            serde_json::json!({ "error": "missing" }).to_string(),
        )]);
        let transport = DataFabricTransport::new(&server.base_url).with_policy(TransportPolicy {
            timeout: Duration::from_secs(2),
            max_retries: 2,
            retry_backoff: Duration::from_millis(0),
        });

        let error = transport
            .send(&TransportRequest::get("/packs/sha256:missing"))
            .unwrap_err();

        assert_eq!(error.attempts(), 1);
        assert_eq!(server.finish().len(), 1);
    }

    #[test]
    fn post_requests_are_not_retried() {
        let server = spawn_server(vec![(
            503,
            "/packs/sha256:abc",
            serde_json::json!({ "error": "try later" }).to_string(),
        )]);
        let transport = DataFabricTransport::new(&server.base_url).with_policy(TransportPolicy {
            timeout: Duration::from_secs(2),
            max_retries: 2,
            retry_backoff: Duration::from_millis(0),
        });

        let error = transport
            .send(&TransportRequest::post(
                "/packs/sha256:abc",
                serde_json::json!({ "pack_id": "sha256:abc" }),
            ))
            .unwrap_err();

        assert_eq!(error.attempts(), 1);
        assert_eq!(server.finish().len(), 1);
    }

    #[test]
    fn successful_non_json_response_is_decode_error() {
        let server = spawn_server(vec![(200, "/packs/sha256:abc", "stored".to_string())]);
        let transport =
            DataFabricTransport::new(&server.base_url).with_timeout(Duration::from_secs(2));

        let error = transport
            .send(&TransportRequest::get("/packs/sha256:abc"))
            .unwrap_err();

        assert!(matches!(error, TransportError::Decode { attempts: 1, .. }));
        assert_eq!(server.finish().len(), 1);
    }

    #[test]
    fn server_failures_return_transport_error() {
        let server = spawn_server(vec![(
            404,
            "/packs/sha256:missing",
            serde_json::json!({ "error": "missing" }).to_string(),
        )]);
        let transport =
            DataFabricTransport::new(&server.base_url).with_timeout(Duration::from_secs(2));
        let error = transport
            .send(&TransportRequest::get("/packs/sha256:missing"))
            .unwrap_err();

        assert_eq!(
            error,
            TransportError::Server {
                status: 404,
                body: Some(serde_json::json!({ "error": "missing" })),
                attempts: 1,
            }
        );
        let envelope = refusal_for_transport("pull", &error);
        assert_eq!(envelope.refusal.code, "E_IO");
        assert_eq!(envelope.refusal.detail.as_ref().unwrap()["status"], 404);
        assert_eq!(envelope.refusal.detail.as_ref().unwrap()["attempts"], 1);
        let _ = server.finish();
    }

    #[test]
    fn transport_policy_env_knobs_are_parsed() {
        let policy = transport_policy_from_env(|key| match key {
            DATA_FABRIC_TIMEOUT_SECS_ENV => Some("5".to_string()),
            DATA_FABRIC_RETRIES_ENV => Some("4".to_string()),
            DATA_FABRIC_RETRY_BACKOFF_MS_ENV => Some("25".to_string()),
            _ => None,
        })
        .unwrap();

        assert_eq!(policy.timeout, Duration::from_secs(5));
        assert_eq!(policy.max_retries, 4);
        assert_eq!(policy.retry_backoff, Duration::from_millis(25));
    }

    #[test]
    fn invalid_transport_policy_env_refuses() {
        let error = transport_policy_from_env(|key| match key {
            DATA_FABRIC_RETRIES_ENV => Some("not-a-number".to_string()),
            _ => None,
        })
        .unwrap_err();

        assert_eq!(error.refusal.code, "E_IO");
        assert_eq!(
            error.refusal.detail.as_ref().unwrap()["env"],
            DATA_FABRIC_RETRIES_ENV
        );
    }

    #[test]
    fn deferred_network_refusal_is_structured() {
        let envelope = deferred_network_refusal("push");
        assert_eq!(envelope.refusal.code, "E_IO");
        assert_eq!(
            envelope.refusal.detail.as_ref().unwrap()["status"],
            "deferred"
        );
    }
}
