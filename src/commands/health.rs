use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use serde_json::json;

pub async fn check(client: &HaClient, url: &str, mode: OutputMode) -> Result<String, AppError> {
    let config = client.get_config().await?;
    format_output(
        &json!({
            "status": "connected",
            "version": config.get("version"),
            "location": config.get("location_name"),
            "url": url,
        }),
        mode,
    )
}
