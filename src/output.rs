use crate::error::AppError;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputMode {
    Json,
    Compact,
    Human,
}

impl OutputMode {
    pub fn from_flags(human: bool, compact: bool) -> Self {
        if human {
            OutputMode::Human
        } else if compact {
            OutputMode::Compact
        } else {
            OutputMode::Json
        }
    }

    /// Auto-detect output mode based on context:
    /// - LLM agent (CLAUDECODE=1) → Compact
    /// - Interactive terminal → Human
    /// - Piped/scripted → JSON
    pub fn auto_detect(human: bool, compact: bool, no_compact: bool) -> Self {
        if human {
            return OutputMode::Human;
        }
        if no_compact {
            return OutputMode::from_flags(human, compact);
        }
        if compact || std::env::var("CLAUDECODE").as_deref() == Ok("1") {
            return OutputMode::Compact;
        }
        // Default to human when stdout is a terminal, JSON when piped
        if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            OutputMode::Human
        } else {
            OutputMode::Json
        }
    }
}

pub fn format_output(data: &Value, mode: OutputMode) -> Result<String, AppError> {
    match mode {
        OutputMode::Json => Ok(serde_json::to_string_pretty(data)?),
        OutputMode::Compact => {
            if let Some(s) = data.as_str() {
                Ok(s.to_string())
            } else {
                Ok(serde_json::to_string(data)?)
            }
        }
        OutputMode::Human => Ok(format_human(data)),
    }
}

pub fn format_entity_list(entities: &[Value], mode: OutputMode, limit: Option<usize>) -> Result<String, AppError> {
    let cap = limit.unwrap_or(entities.len());
    let capped = &entities[..cap.min(entities.len())];

    match mode {
        OutputMode::Compact => {
            let mut lines: Vec<String> = capped
                .iter()
                .filter_map(|e| {
                    let id = e.get("entity_id")?.as_str()?;
                    let state = e.get("state")?.as_str().unwrap_or("unknown")
                        .replace(['\t', '\n', '\r'], " ");
                    Some(format!("{id}\t{state}"))
                })
                .collect();
            if entities.len() > cap {
                lines.push(format!("[+{} more]", entities.len() - cap));
            }
            Ok(lines.join("\n"))
        }
        OutputMode::Human => {
            let s = crate::ui::Style::detect();
            Ok(crate::ui::entity_table(capped, "Entities", &s))
        }
        OutputMode::Json => Ok(serde_json::to_string_pretty(capped)?),
    }
}

fn format_human(data: &Value) -> String {
    match data {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr.iter().map(format_human).collect::<Vec<_>>().join("\n"),
        Value::Object(obj) => obj
            .iter()
            .map(|(k, v)| {
                let val = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                format!("{k}: {val}")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_mode_pretty_prints() {
        let data = json!({"key": "value"});
        let output = format_output(&data, OutputMode::Json).unwrap();
        assert!(output.contains('\n'));
        assert!(output.contains("key"));
    }

    #[test]
    fn compact_mode_single_line() {
        let data = json!({"key": "value"});
        let output = format_output(&data, OutputMode::Compact).unwrap();
        assert!(!output.contains('\n'));
        assert!(output.contains("key"));
    }

    #[test]
    fn compact_mode_strings_unwrapped() {
        let data = json!("hello world");
        let output = format_output(&data, OutputMode::Compact).unwrap();
        assert_eq!(output, "hello world");
    }

    #[test]
    fn human_mode_object() {
        let data = json!({"status": "connected", "version": "2024.3"});
        let output = format_output(&data, OutputMode::Human).unwrap();
        assert!(output.contains("status: connected"));
        assert!(output.contains("version: 2024.3"));
    }

    #[test]
    fn human_mode_array() {
        let data = json!([{"name": "a"}, {"name": "b"}]);
        let output = format_output(&data, OutputMode::Human).unwrap();
        assert!(output.contains("name: a"));
        assert!(output.contains("name: b"));
    }

    #[test]
    fn entity_list_compact_tsv() {
        let entities = vec![
            json!({"entity_id": "light.kitchen", "state": "on"}),
            json!({"entity_id": "sensor.temp", "state": "72.5"}),
        ];
        let output = format_entity_list(&entities, OutputMode::Compact, None).unwrap();
        assert_eq!(output, "light.kitchen\ton\nsensor.temp\t72.5");
    }

    #[test]
    fn entity_list_json_mode() {
        let entities = vec![json!({"entity_id": "light.kitchen", "state": "on"})];
        let output = format_entity_list(&entities, OutputMode::Json, None).unwrap();
        assert!(output.contains("light.kitchen"));
        assert!(output.contains('\n')); // pretty printed
    }

    #[test]
    fn auto_detect_claudecode_env() {
        unsafe { std::env::set_var("CLAUDECODE", "1") };
        let mode = OutputMode::auto_detect(false, false, false);
        assert_eq!(mode, OutputMode::Compact);
        unsafe { std::env::remove_var("CLAUDECODE") };
    }

    #[test]
    fn auto_detect_no_compact_overrides() {
        unsafe { std::env::set_var("CLAUDECODE", "1") };
        let mode = OutputMode::auto_detect(false, false, true);
        assert_eq!(mode, OutputMode::Json);
        unsafe { std::env::remove_var("CLAUDECODE") };
    }

    #[test]
    fn auto_detect_human_wins() {
        let mode = OutputMode::auto_detect(true, true, false);
        assert_eq!(mode, OutputMode::Human);
    }

    #[test]
    fn from_flags_defaults_to_json() {
        assert_eq!(OutputMode::from_flags(false, false), OutputMode::Json);
    }

    #[test]
    fn entity_list_compact_truncates_at_limit() {
        let entities: Vec<Value> = (0..100)
            .map(|i| json!({"entity_id": format!("sensor.s{i}"), "state": "on"}))
            .collect();
        let output = format_entity_list(&entities, OutputMode::Compact, Some(5)).unwrap();
        let lines: Vec<&str> = output.split('\n').collect();
        assert_eq!(lines.len(), 6); // 5 entities + 1 footer
        assert!(lines[5].contains("+95 more"));
    }

    #[test]
    fn entity_list_compact_no_truncation_under_limit() {
        let entities = vec![
            json!({"entity_id": "light.a", "state": "on"}),
            json!({"entity_id": "light.b", "state": "off"}),
        ];
        let output = format_entity_list(&entities, OutputMode::Compact, Some(50)).unwrap();
        assert!(!output.contains("+"));
        assert_eq!(output.lines().count(), 2);
    }

    #[test]
    fn entity_list_compact_sanitizes_tsv_fields() {
        let entities = vec![
            json!({"entity_id": "sensor.test", "state": "has\ttab"}),
            json!({"entity_id": "sensor.nl", "state": "has\nnewline"}),
        ];
        let output = format_entity_list(&entities, OutputMode::Compact, None).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2, "newlines in state should not create extra lines");
        for line in &lines {
            assert_eq!(line.matches('\t').count(), 1, "tabs in state should not create extra columns");
        }
    }

    #[test]
    fn entity_list_human_mode_table() {
        let entities = vec![
            json!({"entity_id": "light.kitchen", "state": "on", "attributes": {"friendly_name": "Kitchen"}}),
            json!({"entity_id": "sensor.temp", "state": "72.5", "attributes": {"friendly_name": "Temperature"}}),
        ];
        let output = format_entity_list(&entities, OutputMode::Human, None).unwrap();
        assert!(output.contains("light.kitchen"));
        assert!(output.contains("Kitchen"));
        assert!(output.contains("2 entities"));
    }

    #[test]
    fn entity_list_json_mode_respects_limit() {
        let entities: Vec<Value> = (0..10)
            .map(|i| json!({"entity_id": format!("sensor.s{i}"), "state": "on"}))
            .collect();
        let output = format_entity_list(&entities, OutputMode::Json, Some(3)).unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed.len(), 3);
    }

    #[test]
    fn entity_list_human_mode_respects_limit() {
        let entities: Vec<Value> = (0..10)
            .map(|i| json!({"entity_id": format!("sensor.s{i}"), "state": "on", "attributes": {"friendly_name": format!("Sensor {i}")}}))
            .collect();
        let output = format_entity_list(&entities, OutputMode::Human, Some(3)).unwrap();
        assert!(output.contains("3 entities"));
        assert!(!output.contains("sensor.s9"));
    }

    #[test]
    fn entity_list_compact_no_limit() {
        let entities: Vec<Value> = (0..100)
            .map(|i| json!({"entity_id": format!("sensor.s{i}"), "state": "on"}))
            .collect();
        let output = format_entity_list(&entities, OutputMode::Compact, None).unwrap();
        assert_eq!(output.lines().count(), 100);
    }
}
