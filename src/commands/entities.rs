use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_entity_list, format_output, OutputMode};
use crate::validation::safe_regex;
use serde_json::{json, Value};

pub async fn list(
    client: &HaClient,
    domain: Option<&str>,
    pattern: Option<&str>,
    area: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    let mut states = client.get_states().await?;

    if let Some(d) = domain {
        let prefix = format!("{d}.");
        states.retain(|e| {
            e.get("entity_id")
                .and_then(|id| id.as_str())
                .is_some_and(|id| id.starts_with(&prefix))
        });
    }

    if let Some(p) = pattern {
        let re = safe_regex(p)?;
        states.retain(|e| matches_entity(&re, e));
    }

    if let Some(a) = area {
        let re = safe_regex(a)?;
        states.retain(|e| matches_entity(&re, e));
    }

    if mode == OutputMode::Compact {
        Ok(format_entity_list(&states, mode))
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
        Ok(format_output(&Value::Array(result), mode))
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
        Ok(format_output(&obj, mode))
    } else {
        Ok(format_output(&state, mode))
    }
}

pub async fn search(
    client: &HaClient,
    pattern: &str,
    mode: OutputMode,
) -> Result<String, AppError> {
    let states = client.get_states().await?;
    let re = safe_regex(pattern)?;

    let matches: Vec<Value> = states
        .into_iter()
        .filter(|e| matches_entity(&re, e))
        .collect();

    if mode == OutputMode::Compact {
        Ok(format_entity_list(&matches, mode))
    } else {
        let result: Vec<Value> = matches
            .into_iter()
            .map(|e| {
                json!({
                    "entity_id": e.get("entity_id"),
                    "state": e.get("state"),
                    "friendly_name": e.pointer("/attributes/friendly_name"),
                })
            })
            .collect();
        Ok(format_output(&Value::Array(result), mode))
    }
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
