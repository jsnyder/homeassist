use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use serde_json::json;

fn format_compact_entry(entry: &serde_json::Value) -> String {
    let when = entry.get("when").and_then(|v| v.as_str()).unwrap_or("");
    let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let message = entry.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let state = entry.get("state").and_then(|v| v.as_str()).unwrap_or("");
    if state.is_empty() {
        format!("{when}\t{name}\t{message}")
    } else {
        format!("{when}\t{name}\t{state}\t{message}")
    }
}

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
            .map(|entry| format_compact_entry(entry))
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
    use super::*;
    use serde_json::json;

    #[test]
    fn compact_entry_with_state() {
        let entry = json!({"when": "2024-01-01T12:00:00", "name": "Kitchen Light", "state": "on", "message": ""});
        let line = format_compact_entry(&entry);
        assert_eq!(line, "2024-01-01T12:00:00\tKitchen Light\ton\t");
    }

    #[test]
    fn compact_entry_without_state() {
        let entry = json!({"when": "2024-01-01T12:00:00", "name": "Scene", "state": "", "message": "activated"});
        let line = format_compact_entry(&entry);
        assert_eq!(line, "2024-01-01T12:00:00\tScene\tactivated");
    }
}
