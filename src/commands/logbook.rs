use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{OutputMode, format_output};
use serde_json::json;

fn sanitize_tsv_field(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

fn format_compact_entry(entry: &serde_json::Value) -> String {
    let when = sanitize_tsv_field(entry.get("when").and_then(|v| v.as_str()).unwrap_or(""));
    let name = sanitize_tsv_field(entry.get("name").and_then(|v| v.as_str()).unwrap_or(""));
    let message = sanitize_tsv_field(entry.get("message").and_then(|v| v.as_str()).unwrap_or(""));
    let state = sanitize_tsv_field(entry.get("state").and_then(|v| v.as_str()).unwrap_or(""));
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
    limit: Option<usize>,
) -> Result<String, AppError> {
    let entries = client.get_logbook(entity_id, hours).await?;
    format_logbook_output(entity_id, hours, &entries, mode, limit)
}

fn format_logbook_output(
    entity_id: &str,
    hours: u32,
    entries: &[serde_json::Value],
    mode: OutputMode,
    limit: Option<usize>,
) -> Result<String, AppError> {
    let total = entries.len();
    let cap = limit.unwrap_or(total);

    if mode == OutputMode::Compact {
        let mut lines: Vec<String> = entries.iter().take(cap).map(format_compact_entry).collect();
        if total > cap {
            lines.push(format!("[+{} more]", total - cap));
        }
        Ok(lines.join("\n"))
    } else {
        let capped: Vec<&serde_json::Value> = entries.iter().take(cap).collect();
        format_output(
            &json!({
                "entity_id": entity_id,
                "hours": hours,
                "entries": capped,
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
    fn format_logbook_json_respects_limit() {
        let entries: Vec<serde_json::Value> = (0..10)
            .map(|i| json!({"when": format!("2024-01-01T{i:02}:00:00"), "name": format!("Event {i}"), "state": "on", "message": ""}))
            .collect();
        let output =
            super::format_logbook_output("sensor.test", 24, &entries, OutputMode::Json, Some(3))
                .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["entries"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn compact_entry_with_state() {
        let entry = json!({"when": "2024-01-01T12:00:00", "name": "Kitchen Light", "state": "on", "message": ""});
        let line = format_compact_entry(&entry);
        assert_eq!(line, "2024-01-01T12:00:00\tKitchen Light\ton\t");
    }

    #[test]
    fn compact_entry_sanitizes_tabs_and_newlines() {
        let entry = json!({"when": "2024-01-01T12:00:00", "name": "Bad\tName", "state": "on", "message": "has\nnewline"});
        let line = format_compact_entry(&entry);
        assert!(!line.contains('\n'), "newlines in fields corrupt TSV rows");
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 4, "tabs in fields corrupt TSV column count");
    }

    #[test]
    fn compact_entry_sanitizes_when_and_state_fields() {
        let entry =
            json!({"when": "2024\t01\t01", "name": "Light", "state": "on\noff", "message": ""});
        let line = format_compact_entry(&entry);
        assert!(!line.contains('\n'), "newlines in state corrupt TSV rows");
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            4,
            "tabs in when/state corrupt TSV column count"
        );
    }

    #[test]
    fn compact_entry_without_state() {
        let entry = json!({"when": "2024-01-01T12:00:00", "name": "Scene", "state": "", "message": "activated"});
        let line = format_compact_entry(&entry);
        assert_eq!(line, "2024-01-01T12:00:00\tScene\tactivated");
    }
}
