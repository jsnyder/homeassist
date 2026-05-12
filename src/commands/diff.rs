use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use serde_json::{json, Value};

/// Show entities that changed state in the last N hours.
/// Compares current state with the first recorded state in the history window.
pub async fn since(
    client: &HaClient,
    hours: u32,
    domain: Option<&str>,
    mode: OutputMode,
    limit: Option<usize>,
) -> Result<String, AppError> {
    let states = client.get_states().await?;

    let mut changed: Vec<Value> = Vec::new();

    for entity in &states {
        let entity_id = match entity.get("entity_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => continue,
        };

        // Filter by domain if specified
        if let Some(d) = domain
            && !entity_id.starts_with(&format!("{d}."))
        {
            continue;
        }

        let current_state = entity
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Check last_changed to see if it changed within the window
        let last_changed = entity
            .get("last_changed")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !last_changed.is_empty() && is_within_hours(last_changed, hours) {
            changed.push(json!({
                "entity_id": entity_id,
                "state": current_state,
                "last_changed": last_changed,
            }));
        }
    }

    format_since_output(&changed, hours, mode, limit)
}

fn format_since_output(
    changed: &[Value],
    hours: u32,
    mode: OutputMode,
    limit: Option<usize>,
) -> Result<String, AppError> {
    let total = changed.len();
    let cap = limit.unwrap_or(total);

    if mode == OutputMode::Compact {
        let mut lines: Vec<String> = changed
            .iter()
            .take(cap)
            .filter_map(|e| {
                let id = e.get("entity_id")?.as_str()?;
                let state = e.get("state")?.as_str()?;
                let changed_at = e.get("last_changed")?.as_str()?;
                Some(format!("{id}\t{state}\t{changed_at}"))
            })
            .collect();
        if total > cap {
            lines.push(format!("[+{} more]", total - cap));
        }
        Ok(lines.join("\n"))
    } else {
        let capped: Vec<&Value> = changed.iter().take(cap).collect();
        format_output(
            &json!({
                "hours": hours,
                "changed_count": total,
                "entities": capped,
            }),
            mode,
        )
    }
}

/// Check if an ISO 8601 timestamp is within the last N hours.
/// Simple string-based comparison — works because ISO 8601 sorts lexicographically.
fn is_within_hours(timestamp: &str, hours: u32) -> bool {
    let cutoff = match crate::client::chrono_offset_public(hours) {
        Ok(c) => c,
        Err(_) => return false,
    };
    timestamp >= cutoff.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_within_hours_recent() {
        // A timestamp from "now" should be within any window
        let now = crate::client::chrono_offset_public(0).unwrap();
        assert!(is_within_hours(&now, 1));
    }

    #[test]
    fn format_since_json_respects_limit() {
        let changed: Vec<Value> = (0..10)
            .map(|i| json!({"entity_id": format!("sensor.s{i}"), "state": "on", "last_changed": "2025-01-01T00:00:00"}))
            .collect();
        let output = super::format_since_output(&changed, 1, OutputMode::Json, Some(3)).unwrap();
        let parsed: Value = serde_json::from_str(&output).unwrap();
        let entities = parsed["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 3);
        assert_eq!(parsed["changed_count"], 10);
    }

    #[test]
    fn format_since_compact_respects_limit() {
        let changed: Vec<Value> = (0..10)
            .map(|i| json!({"entity_id": format!("sensor.s{i}"), "state": "on", "last_changed": "2025-01-01T00:00:00"}))
            .collect();
        let output = super::format_since_output(&changed, 1, OutputMode::Compact, Some(3)).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 4); // 3 entities + [+7 more]
        assert!(lines[3].contains("+7 more"));
    }

    #[test]
    fn is_within_hours_old() {
        // A very old timestamp should not be within 1 hour
        assert!(!is_within_hours("2020-01-01T00:00:00", 1));
    }
}
