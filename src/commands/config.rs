use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use crate::validation::validate_reload_component;
use serde_json::json;

pub async fn check(client: &HaClient, mode: OutputMode) -> Result<String, AppError> {
    let result = client.check_config().await?;
    Ok(format_output(&result, mode))
}

pub async fn reload(
    client: &HaClient,
    component: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    let target = validate_reload_component(component)?;
    let mut reloaded: Vec<&str> = Vec::new();

    if target == "automations" || target == "all" {
        client
            .call_service("automation", "reload", json!({}))
            .await?;
        reloaded.push("automations");
    }
    if target == "scripts" || target == "all" {
        client.call_service("script", "reload", json!({})).await?;
        reloaded.push("scripts");
    }
    if target == "scenes" || target == "all" {
        client.call_service("scene", "reload", json!({})).await?;
        reloaded.push("scenes");
    }

    Ok(format_output(
        &json!({ "success": true, "reloaded": reloaded }),
        mode,
    ))
}
