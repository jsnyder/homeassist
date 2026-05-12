use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_entity_list, format_output, OutputMode};
use crate::ui;
use crate::validation::safe_regex;
use serde_json::{json, Value};

pub async fn list(
    client: &HaClient,
    domain: Option<&str>,
    pattern: Option<&str>,
    name: Option<&str>,
    state: Option<&str>,
    mode: OutputMode,
    limit: Option<usize>,
) -> Result<String, AppError> {
    let human = mode == OutputMode::Human;
    let mut states =
        ui::with_spinner("Loading entities\u{2026}", human, client.get_states()).await?;

    if let Some(d) = domain {
        let prefix = format!("{d}.");
        states.retain(|e| {
            e.get("entity_id")
                .and_then(|id| id.as_str())
                .is_some_and(|id| id.starts_with(&prefix))
        });
    }

    if let Some(s) = state {
        filter_by_state(&mut states, s);
    }

    if let Some(p) = pattern {
        let re = safe_regex(p, true)?;
        states.retain(|e| matches_entity(&re, e));
    }

    if let Some(n) = name {
        let re = safe_regex(n, true)?;
        states.retain(|e| matches_entity(&re, e));
    }

    if mode == OutputMode::Compact {
        format_entity_list(&states, mode, limit)
    } else {
        apply_limit(&mut states, limit);
        if mode == OutputMode::Human {
            let s = ui::Style::detect();
            let title = match domain {
                Some(d) => format!("Entities \u{2014} {d}"),
                None => "Entities".to_string(),
            };
            Ok(ui::entity_table(&states, &title, &s))
        } else {
            let result: Vec<Value> = states
                .into_iter()
                .map(|e| {
                    json!({
                        "entity_id": e.get("entity_id"),
                        "state": e.get("state"),
                        "friendly_name": e.pointer("/attributes/friendly_name"),
                        "last_changed": e.get("last_changed"),
                    })
                })
                .collect();
            format_output(&Value::Array(result), mode)
        }
    }
}

pub async fn get(
    client: &HaClient,
    entity_id: &str,
    mode: OutputMode,
) -> Result<String, AppError> {
    let state = client.get_state(entity_id).await?;

    if mode == OutputMode::Compact {
        // Strip verbose fields for token savings
        let mut obj = state;
        if let Some(map) = obj.as_object_mut() {
            map.remove("context");
            map.remove("last_reported");
            map.remove("last_updated");
        }
        format_output(&obj, mode)
    } else {
        format_output(&state, mode)
    }
}

pub async fn search(
    client: &HaClient,
    pattern: &str,
    mode: OutputMode,
    limit: Option<usize>,
) -> Result<String, AppError> {
    let states = client.get_states().await?;
    let re = safe_regex(pattern, true)?;

    let matches: Vec<Value> = states
        .into_iter()
        .filter(|e| matches_entity(&re, e))
        .collect();

    if mode == OutputMode::Compact {
        format_entity_list(&matches, mode, limit)
    } else {
        let mut capped = matches;
        apply_limit(&mut capped, limit);
        if mode == OutputMode::Human {
            let s = ui::Style::detect();
            let title = format!("Search \u{2014} {pattern}");
            Ok(ui::entity_table(&capped, &title, &s))
        } else {
            let result: Vec<Value> = capped
                .into_iter()
                .map(|e| {
                    json!({
                        "entity_id": e.get("entity_id"),
                        "state": e.get("state"),
                        "friendly_name": e.pointer("/attributes/friendly_name"),
                    })
                })
                .collect();
            format_output(&Value::Array(result), mode)
        }
    }
}

fn apply_limit(entities: &mut Vec<Value>, limit: Option<usize>) {
    if let Some(cap) = limit {
        entities.truncate(cap);
    }
}

fn filter_by_state(entities: &mut Vec<Value>, state: &str) {
    entities.retain(|e| {
        e.get("state")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s == state)
    });
}

fn matches_entity(re: &regex::Regex, entity: &Value) -> bool {
    let id = entity
        .get("entity_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let name = entity
        .pointer("/attributes/friendly_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    re.is_match(id) || re.is_match(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entities() -> Vec<Value> {
        vec![
            json!({"entity_id": "light.kitchen", "state": "on", "attributes": {"friendly_name": "Kitchen Light"}}),
            json!({"entity_id": "light.bedroom", "state": "off", "attributes": {"friendly_name": "Bedroom Light"}}),
            json!({"entity_id": "sensor.temp", "state": "72.5", "attributes": {"friendly_name": "Temperature"}}),
            json!({"entity_id": "binary_sensor.door", "state": "unavailable", "attributes": {"friendly_name": "Front Door"}}),
            json!({"entity_id": "switch.fan", "state": "on", "attributes": {"friendly_name": "Fan"}}),
        ]
    }

    #[test]
    fn apply_limit_truncates() {
        let mut entities = test_entities();
        super::apply_limit(&mut entities, Some(2));
        assert_eq!(entities.len(), 2);
    }

    #[test]
    fn apply_limit_none_keeps_all() {
        let mut entities = test_entities();
        super::apply_limit(&mut entities, None);
        assert_eq!(entities.len(), 5);
    }

    #[test]
    fn filter_by_state_exact_match() {
        let mut entities = test_entities();
        filter_by_state(&mut entities, "on");
        assert_eq!(entities.len(), 2);
        assert!(entities.iter().all(|e| e["state"] == "on"));
    }

    #[test]
    fn filter_by_state_unavailable_does_not_match_available() {
        let mut entities = test_entities();
        filter_by_state(&mut entities, "unavailable");
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0]["entity_id"], "binary_sensor.door");
    }

    #[test]
    fn filter_by_state_no_matches_returns_empty() {
        let mut entities = test_entities();
        filter_by_state(&mut entities, "unknown");
        assert!(entities.is_empty());
    }
}
