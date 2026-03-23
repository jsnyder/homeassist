use crate::error::AppError;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

impl std::fmt::Debug for HaWebSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HaWebSocket")
            .field("msg_id", &self.msg_id.load(Ordering::Relaxed))
            .finish()
    }
}

pub struct HaWebSocket {
    write: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    read: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    msg_id: AtomicU64,
}

impl HaWebSocket {
    pub async fn connect(base_url: &str, token: &str) -> Result<Self, AppError> {
        let ws_url = base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        let ws_url = format!("{ws_url}/api/websocket");

        let (ws_stream, _) = connect_async(&ws_url).await.map_err(|e| {
            AppError::Other(format!("WebSocket connection failed: {e}"))
        })?;

        let (write, read) = ws_stream.split();
        let mut ws = Self {
            write,
            read,
            msg_id: AtomicU64::new(1),
        };

        // Expect auth_required
        let msg = ws.recv().await?;
        if msg.get("type").and_then(|v| v.as_str()) != Some("auth_required") {
            return Err(AppError::Other(
                "Unexpected WebSocket handshake".to_string(),
            ));
        }

        // Send auth
        ws.send_raw(&json!({
            "type": "auth",
            "access_token": token,
        }))
        .await?;

        // Expect auth_ok
        let msg = ws.recv().await?;
        match msg.get("type").and_then(|v| v.as_str()) {
            Some("auth_ok") => {}
            Some("auth_invalid") => {
                let reason = msg
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("invalid token");
                return Err(AppError::Auth(reason.to_string()));
            }
            _ => {
                return Err(AppError::Other(
                    "Unexpected WebSocket auth response".to_string(),
                ));
            }
        }

        Ok(ws)
    }

    pub async fn command(&mut self, msg_type: &str) -> Result<Value, AppError> {
        let id = self.msg_id.fetch_add(1, Ordering::Relaxed);
        self.send_raw(&json!({
            "id": id,
            "type": msg_type,
        }))
        .await?;

        loop {
            let msg = self.recv().await?;
            if msg.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if msg.get("success").and_then(|v| v.as_bool()) == Some(true) {
                    return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
                } else {
                    let error = msg
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error");
                    return Err(AppError::Other(format!(
                        "WebSocket command failed: {error}"
                    )));
                }
            }
        }
    }

    pub async fn command_with_data(
        &mut self,
        msg_type: &str,
        data: Value,
    ) -> Result<Value, AppError> {
        let id = self.msg_id.fetch_add(1, Ordering::Relaxed);
        let mut msg = data;
        if let Some(obj) = msg.as_object_mut() {
            obj.insert("id".to_string(), json!(id));
            obj.insert("type".to_string(), json!(msg_type));
        } else {
            return Err(AppError::Other("data must be a JSON object".to_string()));
        }
        self.send_raw(&msg).await?;

        loop {
            let resp = self.recv().await?;
            if resp.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if resp.get("success").and_then(|v| v.as_bool()) == Some(true) {
                    return Ok(resp.get("result").cloned().unwrap_or(Value::Null));
                } else {
                    let error = resp
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error");
                    return Err(AppError::Other(format!(
                        "WebSocket command failed: {error}"
                    )));
                }
            }
        }
    }

    pub async fn close(mut self) {
        let _ = self.write.close().await;
    }

    async fn send_raw(&mut self, msg: &Value) -> Result<(), AppError> {
        let text = serde_json::to_string(msg)?;
        self.write
            .send(Message::Text(text.into()))
            .await
            .map_err(|e| AppError::Other(format!("WebSocket send failed: {e}")))
    }

    async fn recv(&mut self) -> Result<Value, AppError> {
        loop {
            let next = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                self.read.next(),
            )
            .await
            .map_err(|_| AppError::Other("WebSocket receive timed out".to_string()))?;

            match next {
                Some(Ok(Message::Text(text))) => {
                    return serde_json::from_str(&text).map_err(|e| {
                        AppError::Other(format!("WebSocket JSON parse failed: {e}"))
                    });
                }
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
                Some(Ok(Message::Close(_))) => {
                    return Err(AppError::Other("WebSocket closed by server".to_string()));
                }
                Some(Err(e)) => {
                    return Err(AppError::Other(format!("WebSocket error: {e}")));
                }
                None => {
                    return Err(AppError::Other("WebSocket stream ended".to_string()));
                }
                _ => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::protocol::Message;

    /// Start a local WebSocket server that speaks the HA auth protocol.
    /// Returns the port and a join handle. The server accepts one connection,
    /// performs auth handshake, then processes commands.
    async fn start_ha_mock(accept_auth: bool) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();
            let (mut write, mut read) = ws_stream.split();

            macro_rules! ws_send {
                ($w:expr, $v:expr) => {
                    $w.send(Message::Text($v.to_string().into())).await.unwrap()
                };
            }

            // Step 1: Send auth_required
            ws_send!(write, json!({"type": "auth_required", "ha_version": "2024.3.0"}));

            // Step 2: Read auth message
            let msg = read.next().await.unwrap().unwrap();
            let auth: serde_json::Value = serde_json::from_str(&msg.to_text().unwrap()).unwrap();
            assert_eq!(auth["type"], "auth");

            // Step 3: Send auth_ok or auth_invalid
            if accept_auth {
                ws_send!(write, json!({"type": "auth_ok", "ha_version": "2024.3.0"}));
            } else {
                ws_send!(write, json!({"type": "auth_invalid", "message": "Invalid access token"}));
                return;
            }

            // Step 4: Handle commands
            while let Some(Ok(msg)) = read.next().await {
                if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(msg.to_text().unwrap()) {
                    let id = cmd["id"].as_u64().unwrap();
                    let msg_type = cmd["type"].as_str().unwrap();

                    let result = match msg_type {
                        "system_log/list" => json!([
                            {
                                "name": "homeassistant.core",
                                "message": ["Test error message"],
                                "level": "ERROR",
                                "source": ["core.py", 123],
                                "timestamp": 1710000000.0,
                                "count": 1,
                                "first_occurred": 1710000000.0,
                            }
                        ]),
                        "get_config" => json!({"version": "2024.3.0"}),
                        _ => json!(null),
                    };

                    ws_send!(write, json!({
                        "id": id,
                        "type": "result",
                        "success": true,
                        "result": result,
                    }));
                }
            }
        });

        (port, handle)
    }

    #[tokio::test]
    async fn connects_and_authenticates() {
        let (port, server) = start_ha_mock(true).await;
        let url = format!("http://127.0.0.1:{port}");

        let ws = HaWebSocket::connect(&url, "test-token").await;
        assert!(ws.is_ok(), "Should connect and authenticate successfully");

        let ws = ws.unwrap();
        ws.close().await;
        server.abort();
    }

    #[tokio::test]
    async fn auth_failure_returns_error() {
        let (port, server) = start_ha_mock(false).await;
        let url = format!("http://127.0.0.1:{port}");

        let result = HaWebSocket::connect(&url, "bad-token").await;
        assert!(result.is_err(), "Should return error on auth failure");

        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Invalid access token") || msg.contains("auth"),
            "Error should mention auth failure: {msg}"
        );

        server.abort();
    }

    #[tokio::test]
    async fn sends_command_and_receives_result() {
        let (port, server) = start_ha_mock(true).await;
        let url = format!("http://127.0.0.1:{port}");

        let mut ws = HaWebSocket::connect(&url, "test-token").await.unwrap();
        let result = ws.command("get_config").await.unwrap();

        assert_eq!(result["version"], "2024.3.0");

        ws.close().await;
        server.abort();
    }

    #[tokio::test]
    async fn system_log_list_returns_entries() {
        let (port, server) = start_ha_mock(true).await;
        let url = format!("http://127.0.0.1:{port}");

        let mut ws = HaWebSocket::connect(&url, "test-token").await.unwrap();
        let result = ws.command("system_log/list").await.unwrap();

        let entries = result.as_array().expect("Should return array of log entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["level"], "ERROR");
        assert_eq!(entries[0]["name"], "homeassistant.core");

        ws.close().await;
        server.abort();
    }

    #[tokio::test]
    async fn command_with_data_includes_extra_fields() {
        let (port, server) = start_ha_mock(true).await;
        let url = format!("http://127.0.0.1:{port}");

        let mut ws = HaWebSocket::connect(&url, "test-token").await.unwrap();
        let result = ws
            .command_with_data("get_config", json!({"extra": "field"}))
            .await
            .unwrap();

        assert_eq!(result["version"], "2024.3.0");

        ws.close().await;
        server.abort();
    }
}
