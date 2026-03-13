use crate::refusal::{RefusalCode, RefusalEnvelope};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Network { message: String },
    Server { status: u16, body: Option<Value> },
    Decode { message: String },
}

#[derive(Debug, Clone)]
pub struct DataFabricTransport {
    base_url: String,
    timeout: Duration,
}

impl DataFabricTransport {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn send(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        let url = build_url(&self.base_url, &request.path);
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(self.timeout)
            .timeout_read(self.timeout)
            .timeout_write(self.timeout)
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
                    decode_body(response).map_err(|message| TransportError::Decode { message })?;
                Ok(TransportResponse { status, body })
            }
            Err(ureq::Error::Status(status, response)) => OkErr::Err(TransportError::Server {
                status,
                body: decode_body(response).unwrap_or_else(|message| Some(Value::String(message))),
            }),
            Err(ureq::Error::Transport(error)) => Err(TransportError::Network {
                message: error.to_string(),
            }),
        }
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
        })?;
        serde_json::from_value(body).map_err(|error| TransportError::Decode {
            message: format!("Failed to decode transport response: {error}"),
        })
    }
}

pub fn refusal_for_transport(action: &str, error: &TransportError) -> RefusalEnvelope {
    let (message, detail) = match error {
        TransportError::Network { message } => (
            format!("pack {action} transport failure: {message}"),
            serde_json::json!({
                "action": action,
                "kind": "network",
                "message": message,
            }),
        ),
        TransportError::Server { status, body } => (
            format!("pack {action} server failure: HTTP {status}"),
            serde_json::json!({
                "action": action,
                "kind": "server",
                "status": status,
                "body": body,
            }),
        ),
        TransportError::Decode { message } => (
            format!("pack {action} transport decode failure: {message}"),
            serde_json::json!({
                "action": action,
                "kind": "decode",
                "message": message,
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

fn decode_body(response: ureq::Response) -> Result<Option<Value>, String> {
    let text = response.into_string().map_err(|error| error.to_string())?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text)
        .map(Some)
        .or(Ok(Some(Value::String(text))))
}

type OkErr<T, E> = Result<T, E>;

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
            }
        );
        let envelope = refusal_for_transport("pull", &error);
        assert_eq!(envelope.refusal.code, "E_IO");
        assert_eq!(envelope.refusal.detail.as_ref().unwrap()["status"], 404);
        let _ = server.finish();
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
