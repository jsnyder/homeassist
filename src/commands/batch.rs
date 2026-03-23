use crate::client::HaClient;
use crate::error::AppError;
use crate::output::OutputMode;
use serde_json::{json, Value};
use std::io::BufRead;

/// Execute multiple commands from JSONL input (stdin or file).
/// Each line is a JSON object with "command" and optional "args" fields.
///
/// Format: {"command":"entities.get","args":{"entity_id":"light.kitchen"}}
/// Supported commands:
///   entities.get, entities.list, entities.search
///   services.call, services.list
///   templates.render
///   health
pub async fn run(
    client: &HaClient,
    url: &str,
    input: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    let lines: Vec<String> = if let Some(path) = input {
        let content = std::fs::read_to_string(path)
            .map_err(|e| AppError::Other(format!("Failed to read {path}: {e}")))?;
        content.lines().map(String::from).collect()
    } else {
        let stdin = std::io::stdin();
        stdin.lock().lines().map_while(Result::ok).collect()
    };

    let mut results: Vec<Value> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let cmd: Value = serde_json::from_str(line).map_err(|e| {
            AppError::Other(format!("Invalid JSON on line {}: {e}", i + 1))
        })?;

        let command = cmd
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::Other(format!("Missing 'command' field on line {}", i + 1))
            })?;

        let args = cmd.get("args").cloned().unwrap_or(json!({}));
        let result = execute_command(client, url, command, &args, mode).await;

        results.push(json!({
            "command": command,
            "line": i + 1,
            "success": result.is_ok(),
            "result": match &result {
                Ok(output) => {
                    // Try to parse output as JSON, fall back to string
                    serde_json::from_str::<Value>(output).unwrap_or(Value::String(output.clone()))
                }
                Err(e) => Value::String(e.to_string()),
            },
        }));
    }

    let output = json!({
        "total": results.len(),
        "succeeded": results.iter().filter(|r| r["success"] == true).count(),
        "failed": results.iter().filter(|r| r["success"] == false).count(),
        "results": results,
    });

    crate::output::format_output(&output, mode)
}

async fn execute_command(
    client: &HaClient,
    url: &str,
    command: &str,
    args: &Value,
    mode: OutputMode,
) -> Result<String, AppError> {
    match command {
        "entities.get" => {
            let entity_id = args
                .get("entity_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Other("Missing 'entity_id' arg".into()))?;
            crate::commands::entities::get(client, entity_id, mode).await
        }
        "entities.list" => {
            let domain = args.get("domain").and_then(|v| v.as_str());
            let pattern = args.get("pattern").and_then(|v| v.as_str());
            let name = args.get("name").and_then(|v| v.as_str());
            let state = args.get("state").and_then(|v| v.as_str());
            crate::commands::entities::list(client, domain, pattern, name, state, mode).await
        }
        "entities.search" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Other("Missing 'pattern' arg".into()))?;
            crate::commands::entities::search(client, pattern, mode).await
        }
        "services.list" => {
            let domain = args.get("domain").and_then(|v| v.as_str());
            crate::commands::services::list(client, domain, mode).await
        }
        "services.call" => {
            let service = args
                .get("service")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Other("Missing 'service' arg".into()))?;
            let data = args.get("data").and_then(|v| v.as_str());
            let target = args.get("target").and_then(|v| v.as_str());
            crate::commands::services::call(client, service, data, target, mode).await
        }
        "templates.render" => {
            let template = args
                .get("template")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Other("Missing 'template' arg".into()))?;
            crate::commands::templates::render(client, template, mode).await
        }
        "health" => crate::commands::health::check(client, url, mode).await,
        _ => Err(AppError::Other(format!("Unknown batch command: {command}"))),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn parse_batch_line() {
        let line = r#"{"command":"entities.get","args":{"entity_id":"light.kitchen"}}"#;
        let cmd: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(cmd["command"], "entities.get");
        assert_eq!(cmd["args"]["entity_id"], "light.kitchen");
    }

    #[test]
    fn parse_batch_line_no_args() {
        let line = r#"{"command":"health"}"#;
        let cmd: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(cmd["command"], "health");
        let args = cmd.get("args").cloned().unwrap_or(json!({}));
        assert!(args.is_object());
    }
}
