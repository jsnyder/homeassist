use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use crate::validation::parse_json_option;
use serde_json::json;

pub async fn fire(
    client: &HaClient,
    event_type: &str,
    data_json: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    let data = if let Some(d) = data_json {
        parse_json_option(d, "data")?
    } else {
        json!({})
    };

    let result = client.fire_event(event_type, data).await?;
    format_output(
        &json!({
            "success": true,
            "event_type": event_type,
            "result": result,
        }),
        mode,
    )
}
