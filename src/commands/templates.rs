use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use serde_json::json;

pub async fn render(
    client: &HaClient,
    template: &str,
    mode: OutputMode,
) -> Result<String, AppError> {
    let result = client.render_template(template).await?;
    format_output(&json!({ "template": template, "result": result }), mode)
}
