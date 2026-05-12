use crate::client::HaClient;
use crate::error::AppError;
use crate::output::OutputMode;
use serde_json::{json, Value};
use std::io::BufRead;

fn collect_lines<I>(iter: I) -> Result<Vec<String>, AppError>
where
    I: Iterator<Item = Result<String, std::io::Error>>,
{
    iter.map(|r| r.map_err(|e| AppError::Other(format!("Failed to read input: {e}"))))
        .collect()
}

fn parse_batch_command(line: &str) -> Result<(String, Value), AppError> {
    let cmd: Value = serde_json::from_str(line)
        .map_err(|e| AppError::Other(format!("Invalid JSON: {e}")))?;
    let command = cmd
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Other("Missing 'command' field".into()))?
        .to_string();
    let args = cmd.get("args").cloned().unwrap_or(json!({}));
    Ok((command, args))
}

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
        collect_lines(stdin.lock().lines())?
    };

    let mut results: Vec<Value> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parsed = parse_batch_command(line);
        let (command, args) = match parsed {
            Ok(v) => v,
            Err(e) => {
                results.push(json!({
                    "command": null,
                    "line": i + 1,
                    "success": false,
                    "result": format!("Line {}: {e}", i + 1),
                }));
                continue;
            }
        };

        let result = execute_command(client, url, &command, &args, mode).await;

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
            crate::commands::entities::list(client, domain, pattern, name, state, mode, None).await
        }
        "entities.search" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Other("Missing 'pattern' arg".into()))?;
            crate::commands::entities::search(client, pattern, mode, None).await
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
            let data = args.get("data").map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
            let target = args.get("target").map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
            crate::commands::services::call(
                client,
                service,
                data.as_deref(),
                target.as_deref(),
                mode,
            )
            .await
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
    use super::*;

    #[test]
    fn parse_batch_command_with_args() {
        let (cmd, args) = parse_batch_command(
            r#"{"command":"entities.get","args":{"entity_id":"light.kitchen"}}"#,
        ).unwrap();
        assert_eq!(cmd, "entities.get");
        assert_eq!(args["entity_id"], "light.kitchen");
    }

    #[test]
    fn parse_batch_command_no_args_defaults_to_empty_object() {
        let (cmd, args) = parse_batch_command(r#"{"command":"health"}"#).unwrap();
        assert_eq!(cmd, "health");
        assert!(args.is_object());
        assert_eq!(args.as_object().unwrap().len(), 0);
    }

    #[test]
    fn parse_batch_command_missing_command_field_errors() {
        let err = parse_batch_command(r#"{"args":{}}"#).unwrap_err();
        assert!(err.to_string().contains("command"));
    }

    #[test]
    fn parse_batch_command_invalid_json_errors() {
        let err = parse_batch_command("not json").unwrap_err();
        assert!(err.to_string().contains("Invalid JSON"));
    }

    #[test]
    fn collect_lines_propagates_errors() {
        let lines = vec![
            Ok("line1".to_string()),
            Ok("line2".to_string()),
        ];
        let result = collect_lines(lines.into_iter());
        assert_eq!(result.unwrap(), vec!["line1", "line2"]);
    }

    #[test]
    fn collect_lines_returns_error_on_io_failure() {
        let lines: Vec<Result<String, std::io::Error>> = vec![
            Ok("line1".to_string()),
            Err(std::io::Error::new(std::io::ErrorKind::Other, "read failed")),
            Ok("line3".to_string()),
        ];
        let result = collect_lines(lines.into_iter());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("read failed"));
    }
}
