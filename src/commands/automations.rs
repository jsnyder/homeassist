use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_entity_list, format_output, OutputMode};
use serde_json::{json, Value};

pub async fn list(client: &HaClient, mode: OutputMode, limit: Option<usize>) -> Result<String, AppError> {
    let states = client.get_states().await?;
    let automations: Vec<Value> = states
        .into_iter()
        .filter(|e| {
            e.get("entity_id")
                .and_then(|id| id.as_str())
                .is_some_and(|id| id.starts_with("automation."))
        })
        .collect();

    if mode == OutputMode::Compact {
        format_entity_list(&automations, mode, limit)
    } else {
        let result: Vec<Value> = automations
            .into_iter()
            .map(|e| {
                json!({
                    "entity_id": e.get("entity_id"),
                    "state": e.get("state"),
                    "friendly_name": e.pointer("/attributes/friendly_name"),
                    "last_triggered": e.pointer("/attributes/last_triggered"),
                })
            })
            .collect();
        format_output(&Value::Array(result), mode)
    }
}

pub async fn trigger(
    client: &HaClient,
    entity_id: &str,
    mode: OutputMode,
) -> Result<String, AppError> {
    let data = json!({ "entity_id": entity_id });
    client
        .call_service("automation", "trigger", data)
        .await?;
    format_output(
        &json!({ "success": true, "triggered": entity_id }),
        mode,
    )
}

pub async fn scripts_list(client: &HaClient, mode: OutputMode, limit: Option<usize>) -> Result<String, AppError> {
    let states = client.get_states().await?;
    let scripts: Vec<Value> = states
        .into_iter()
        .filter(|e| {
            e.get("entity_id")
                .and_then(|id| id.as_str())
                .is_some_and(|id| id.starts_with("script."))
        })
        .collect();

    if mode == OutputMode::Compact {
        format_entity_list(&scripts, mode, limit)
    } else {
        let result: Vec<Value> = scripts
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

fn validate_script_id(entity_id: &str) -> Result<&str, AppError> {
    entity_id
        .strip_prefix("script.")
        .filter(|name| !name.is_empty())
        .ok_or_else(|| AppError::Other(format!(
            "Invalid script entity ID: \"{entity_id}\". Expected format: script.<name>"
        )))
}

pub async fn scripts_run(
    client: &HaClient,
    entity_id: &str,
    mode: OutputMode,
) -> Result<String, AppError> {
    let script_name = validate_script_id(entity_id)?;
    let data = json!({});
    client
        .call_service("script", script_name, data)
        .await?;
    format_output(
        &json!({ "success": true, "ran": entity_id }),
        mode,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_script_id_with_prefix() {
        assert_eq!(validate_script_id("script.my_script").unwrap(), "my_script");
    }

    #[test]
    fn validate_script_id_without_prefix_errors() {
        assert!(validate_script_id("my_script").is_err());
    }

    #[test]
    fn validate_script_id_rejects_non_script_input() {
        assert!(validate_script_id("reload").is_err());
        assert!(validate_script_id("automation.test").is_err());
        assert!(validate_script_id("turn_on").is_err());
    }

    #[test]
    fn validate_script_id_accepts_valid_script_ids() {
        assert_eq!(validate_script_id("script.my_script").unwrap(), "my_script");
        assert_eq!(validate_script_id("script.morning_routine").unwrap(), "morning_routine");
    }

    #[test]
    fn validate_script_id_rejects_empty_name() {
        assert!(validate_script_id("script.").is_err());
    }
}
