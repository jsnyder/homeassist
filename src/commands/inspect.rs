use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use serde_json::{json, Value};

pub async fn triage(client: &HaClient, mode: OutputMode) -> Result<String, AppError> {
    let states: Vec<Value> = client.get_states().await?;

    let mut unavailable: Vec<Value> = Vec::new();
    let mut unknown: Vec<Value> = Vec::new();
    let mut domain_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for entity in &states {
        let entity_id = entity
            .get("entity_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let state = entity
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Count by domain
        if let Some(domain) = entity_id.split('.').next() {
            *domain_counts.entry(domain.to_string()).or_insert(0) += 1;
        }

        match state {
            "unavailable" => {
                unavailable.push(json!({
                    "entity_id": entity_id,
                    "friendly_name": entity.get("attributes")
                        .and_then(|a| a.get("friendly_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                }));
            }
            "unknown" => {
                unknown.push(json!({
                    "entity_id": entity_id,
                    "friendly_name": entity.get("attributes")
                        .and_then(|a| a.get("friendly_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                }));
            }
            _ => {}
        }
    }

    if mode == OutputMode::Compact {
        let mut lines = Vec::new();
        lines.push(format!(
            "total:{}\tunavailable:{}\tunknown:{}",
            states.len(),
            unavailable.len(),
            unknown.len()
        ));
        if !unavailable.is_empty() {
            lines.push("--- unavailable ---".to_string());
            for e in &unavailable {
                lines.push(format!(
                    "{}\t{}",
                    e["entity_id"].as_str().unwrap_or(""),
                    e["friendly_name"].as_str().unwrap_or("")
                ));
            }
        }
        if !unknown.is_empty() {
            lines.push("--- unknown ---".to_string());
            for e in &unknown {
                lines.push(format!(
                    "{}\t{}",
                    e["entity_id"].as_str().unwrap_or(""),
                    e["friendly_name"].as_str().unwrap_or("")
                ));
            }
        }
        Ok(lines.join("\n"))
    } else {
        // Sort domain counts descending
        let mut domains: Vec<_> = domain_counts.into_iter().collect();
        domains.sort_by(|a, b| b.1.cmp(&a.1));
        let domain_summary: Value = domains
            .into_iter()
            .map(|(k, v)| json!({"domain": k, "count": v}))
            .collect();

        format_output(
            &json!({
                "total_entities": states.len(),
                "unavailable_count": unavailable.len(),
                "unknown_count": unknown.len(),
                "unavailable": unavailable,
                "unknown": unknown,
                "domains": domain_summary,
            }),
            mode,
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn domain_extraction() {
        let entity_id = "sensor.living_room_temperature";
        let domain = entity_id.split('.').next().unwrap();
        assert_eq!(domain, "sensor");
    }
}
