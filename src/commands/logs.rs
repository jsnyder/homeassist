use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use crate::validation::safe_regex;
use crate::ws::HaWebSocket;
use serde_json::{json, Value};

/// Format a single structured log entry from system_log/list into a readable line.
fn format_log_entry(entry: &Value) -> String {
    let level = entry
        .get("level")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN");
    let name = entry
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let message = entry
        .get("message")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let count = entry
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);

    if count > 1 {
        format!("{level} ({name}) [{count}x]: {message}")
    } else {
        format!("{level} ({name}): {message}")
    }
}

/// Format a list of WebSocket log entries into display lines.
fn format_ws_entries(entries: &[Value]) -> Vec<String> {
    entries.iter().map(format_log_entry).collect()
}

pub async fn errors(
    client: &HaClient,
    base_url: &str,
    token: &str,
    tail: Option<usize>,
    pattern: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    // Try WebSocket system_log/list first, fall back to REST
    let mut lines = match fetch_ws_logs(base_url, token).await {
        Ok(entries) => format_ws_entries(&entries),
        Err(_) => {
            // Fallback to REST
            let log = client.get_error_log().await?;
            log.lines().map(|s| s.to_string()).collect()
        }
    };

    if let Some(p) = pattern {
        let re = safe_regex(p, true)?;
        lines.retain(|line| re.is_match(line));
    }

    if let Some(n) = tail {
        let start = lines.len().saturating_sub(n);
        lines = lines[start..].to_vec();
    }

    if mode == OutputMode::Compact {
        Ok(lines.join("\n"))
    } else {
        format_output(
            &json!({
                "lines": lines.len(),
                "log": lines,
            }),
            mode,
        )
    }
}

async fn fetch_ws_logs(base_url: &str, token: &str) -> Result<Vec<Value>, AppError> {
    let mut ws = HaWebSocket::connect(base_url, token).await?;
    let result = ws.command("system_log/list").await?;
    ws.close().await;
    result
        .as_array()
        .cloned()
        .ok_or_else(|| AppError::Other("system_log/list did not return an array".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tail_limits_output() {
        let lines = vec!["a", "b", "c", "d", "e"];
        let n = 3usize;
        let start = lines.len().saturating_sub(n);
        let result = &lines[start..];
        assert_eq!(result, &["c", "d", "e"]);
    }

    #[test]
    fn tail_larger_than_input() {
        let lines = vec!["a", "b"];
        let n = 10usize;
        let start = lines.len().saturating_sub(n);
        let result = &lines[start..];
        assert_eq!(result, &["a", "b"]);
    }

    #[test]
    fn formats_ws_log_entry_to_line() {
        let entry = json!({
            "name": "homeassistant.components.zwave_js",
            "message": ["Node 5 is not responding"],
            "level": "ERROR",
            "source": ["components/zwave_js/node.py", 142],
            "timestamp": 1710000000.0,
            "count": 3,
            "first_occurred": 1709990000.0,
        });

        let line = format_log_entry(&entry);
        assert!(line.contains("ERROR"), "Should include level");
        assert!(line.contains("zwave_js"), "Should include component name");
        assert!(line.contains("Node 5 is not responding"), "Should include message");
    }

    #[test]
    fn formats_ws_log_entry_with_count() {
        let entry = json!({
            "name": "homeassistant.core",
            "message": ["Something broke"],
            "level": "WARNING",
            "source": ["core.py", 50],
            "timestamp": 1710000000.0,
            "count": 7,
            "first_occurred": 1709990000.0,
        });

        let line = format_log_entry(&entry);
        assert!(line.contains("7"), "Should show count when > 1");
    }

    #[test]
    fn formats_ws_entries_to_lines() {
        let entries = vec![
            json!({
                "name": "homeassistant.core",
                "message": ["Error one"],
                "level": "ERROR",
                "source": ["core.py", 10],
                "timestamp": 1710000000.0,
                "count": 1,
                "first_occurred": 1710000000.0,
            }),
            json!({
                "name": "homeassistant.components.mqtt",
                "message": ["Error two"],
                "level": "WARNING",
                "source": ["mqtt/client.py", 20],
                "timestamp": 1710001000.0,
                "count": 1,
                "first_occurred": 1710001000.0,
            }),
        ];

        let lines = format_ws_entries(&entries);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Error one"));
        assert!(lines[1].contains("Error two"));
    }
}
