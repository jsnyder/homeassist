use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{OutputMode, format_output};
use crate::ui;
use serde_json::json;

pub async fn check(client: &HaClient, url: &str, mode: OutputMode) -> Result<String, AppError> {
    let config = ui::with_spinner(
        "Connecting\u{2026}",
        mode == OutputMode::Human,
        client.get_config(),
    )
    .await?;

    if mode == OutputMode::Human {
        let s = ui::Style::detect();
        let version = config
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let location = config
            .get("location_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let w = 12;
        Ok(format!(
            "{}\n\n{}\n{}\n{}\n{}\n",
            s.header("Home Assistant"),
            s.kv("Version", w, version),
            s.kv("Location", w, location),
            s.kv("Status", w, &format!("{}connected{}", s.green, s.reset)),
            s.kv("URL", w, url),
        ))
    } else {
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
}
