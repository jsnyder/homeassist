use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use serde_json::json;

pub async fn get(
    client: &HaClient,
    entity_id: &str,
    hours: u32,
    mode: OutputMode,
) -> Result<String, AppError> {
    let entries = client.get_logbook(entity_id, hours).await?;

    if mode == OutputMode::Compact {
        let lines: Vec<String> = entries
            .iter()
            .map(|entry| {
                let when = entry.get("when").and_then(|v| v.as_str()).unwrap_or("");
                let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let message = entry.get("message").and_then(|v| v.as_str()).unwrap_or("");
                let state = entry.get("state").and_then(|v| v.as_str()).unwrap_or("");
                if state.is_empty() {
                    format!("{when}\t{name}\t{message}")
                } else {
                    format!("{when}\t{name}\t{state}\t{message}")
                }
            })
            .collect();
        Ok(lines.join("\n"))
    } else {
        format_output(
            &json!({
                "entity_id": entity_id,
                "hours": hours,
                "entries": entries,
            }),
            mode,
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn compact_format_with_state() {
        let entry = json!({"when": "2024-01-01T12:00:00", "name": "Kitchen Light", "state": "on", "message": ""});
        let when = entry.get("when").and_then(|v| v.as_str()).unwrap_or("");
        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let state = entry.get("state").and_then(|v| v.as_str()).unwrap_or("");
        let line = format!("{when}\t{name}\t{state}\t");
        assert!(line.contains("Kitchen Light"));
        assert!(line.contains("on"));
    }

    #[test]
    fn compact_format_without_state() {
        let entry = json!({"when": "2024-01-01T12:00:00", "name": "Scene", "state": "", "message": "activated"});
        let state = entry.get("state").and_then(|v| v.as_str()).unwrap_or("");
        assert!(state.is_empty());
    }
}
