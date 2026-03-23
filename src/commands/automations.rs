use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_entity_list, format_output, OutputMode};
use serde_json::{json, Value};

pub async fn list(client: &HaClient, mode: OutputMode) -> Result<String, AppError> {
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
        format_entity_list(&automations, mode)
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

pub async fn scripts_list(client: &HaClient, mode: OutputMode) -> Result<String, AppError> {
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
        format_entity_list(&scripts, mode)
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

fn strip_script_domain(entity_id: &str) -> &str {
    entity_id.strip_prefix("script.").unwrap_or(entity_id)
}

pub async fn scripts_run(
    client: &HaClient,
    entity_id: &str,
    mode: OutputMode,
) -> Result<String, AppError> {
    let script_name = strip_script_domain(entity_id);
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
    fn strip_script_domain_with_prefix() {
        assert_eq!(strip_script_domain("script.my_script"), "my_script");
    }

    #[test]
    fn strip_script_domain_without_prefix() {
        assert_eq!(strip_script_domain("my_script"), "my_script");
    }
}
