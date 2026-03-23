use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use crate::ui;
use regex::Regex;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug)]
struct Finding {
    file: String,
    line: Option<usize>,
    severity: &'static str, // "error" or "warning"
    check: &'static str,
    message: String,
}

pub async fn run(
    client: Option<&HaClient>,
    path: &str,
    check_entities: bool,
    mode: OutputMode,
) -> Result<String, AppError> {
    let path = Path::new(path);
    if !path.exists() {
        return Err(AppError::Other(format!("Path not found: {}", path.display())));
    }

    let yaml_files = find_yaml_files(path)?;
    if yaml_files.is_empty() {
        return Err(AppError::Other(format!(
            "No YAML files found in {}",
            path.display()
        )));
    }

    let mut findings: Vec<Finding> = Vec::new();

    for file_path in &yaml_files {
        let content = std::fs::read_to_string(file_path).map_err(|e| {
            AppError::Other(format!("Failed to read {}: {e}", file_path))
        })?;

        // YAML syntax check
        check_yaml_syntax(file_path, &content, &mut findings);

        // Structural checks (only on valid YAML)
        if serde_yaml::from_str::<serde_yaml::Value>(&content).is_ok() {
            check_duplicate_keys(file_path, &content, &mut findings);
            check_jinja_templates(file_path, &content, &mut findings);
            check_common_errors(file_path, &content, &mut findings);
            check_file_cruft(file_path, &mut findings);
            check_package_exclusions(file_path, &content, &mut findings);
            check_sensor_platforms(file_path, &content, &mut findings);
            check_automation_syntax(file_path, &content, &mut findings);
        }
    }

    // Cross-file duplicate entity check
    check_duplicate_entities_across_files(&yaml_files, &mut findings);

    // Entity reference check against live HA (if client provided and flag set)
    if check_entities
        && let Some(client) = client {
            check_entity_references(client, &yaml_files, &mut findings).await;
        }

    let errors = findings.iter().filter(|f| f.severity == "error").count();
    let warnings = findings.iter().filter(|f| f.severity == "warning").count();

    if mode == OutputMode::Compact {
        let mut lines = Vec::new();
        lines.push(format!(
            "files:{}\terrors:{}\twarnings:{}",
            yaml_files.len(),
            errors,
            warnings
        ));
        for f in &findings {
            let loc = match f.line {
                Some(l) => format!("{}:{}", f.file, l),
                None => f.file.clone(),
            };
            lines.push(format!(
                "{}\t{}\t{}\t{}",
                f.severity, f.check, loc, f.message
            ));
        }
        Ok(lines.join("\n"))
    } else if mode == OutputMode::Human {
        let s = ui::Style::detect();
        let mut out = format!(
            "{}\n\n",
            s.header(&format!(
                "YAML Validation \u{2014} {} files",
                yaml_files.len()
            ))
        );

        // Summary line
        if errors == 0 && warnings == 0 {
            out.push_str(&format!("  {}\n", s.pass("No issues found")));
        } else {
            let mut parts = Vec::new();
            if errors > 0 {
                parts.push(format!("{}{} error{}{}", s.red, errors, if errors == 1 { "" } else { "s" }, s.reset));
            }
            if warnings > 0 {
                parts.push(format!("{}{} warning{}{}", s.yellow, warnings, if warnings == 1 { "" } else { "s" }, s.reset));
            }
            out.push_str(&format!("  {}\n", parts.join(&format!("  {}·{}  ", s.dim, s.reset))));
        }

        // Findings grouped by severity
        if !findings.is_empty() {
            out.push('\n');
            for f in &findings {
                let loc = match f.line {
                    Some(l) => format!("{}:{}", ui::basename(&f.file), l),
                    None => ui::basename(&f.file).to_string(),
                };
                out.push_str(&format!(
                    "  {}{}{}\n",
                    s.dim, loc, s.reset,
                ));
                let icon = if f.severity == "error" {
                    s.fail("")
                } else {
                    s.warn("")
                };
                out.push_str(&format!("  {} {}\n\n", icon, f.message));
            }
        }

        Ok(out)
    } else {
        let finding_json: Vec<_> = findings
            .iter()
            .map(|f| {
                json!({
                    "file": f.file,
                    "line": f.line,
                    "severity": f.severity,
                    "check": f.check,
                    "message": f.message,
                })
            })
            .collect();

        format_output(
            &json!({
                "files_checked": yaml_files.len(),
                "errors": errors,
                "warnings": warnings,
                "passed": errors == 0,
                "findings": finding_json,
            }),
            mode,
        )
    }
}

fn find_yaml_files(path: &Path) -> Result<Vec<String>, AppError> {
    let mut files = Vec::new();

    if path.is_file() {
        files.push(path.to_string_lossy().to_string());
        return Ok(files);
    }

    let pattern = format!("{}/**/*.yaml", path.display());
    for p in glob::glob(&pattern)
        .map_err(|e| AppError::Other(format!("Invalid glob pattern: {e}")))?
        .flatten()
    {
        let s = p.to_string_lossy().to_string();
        // Skip archive/backup/versioned files
        if s.contains("/archive/")
            || s.contains("/.git/")
            || s.contains("_backup")
            || s.ends_with(".disabled")
        {
            continue;
        }
        files.push(s);
    }

    // Also check .yml files
    let pattern_yml = format!("{}/**/*.yml", path.display());
    for p in glob::glob(&pattern_yml)
        .map_err(|e| AppError::Other(format!("Invalid glob pattern: {e}")))?.flatten()
    {
        let s = p.to_string_lossy().to_string();
        if s.contains("/archive/") || s.contains("/.git/") || s.ends_with(".disabled") {
            continue;
        }
        files.push(s);
    }

    Ok(files)
}

fn check_yaml_syntax(file: &str, content: &str, findings: &mut Vec<Finding>) {
    if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(content) {
        let line = e.location().map(|l| l.line());
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: "error",
            check: "yaml_syntax",
            message: format!("Invalid YAML: {e}"),
        });
    }
}

fn check_duplicate_keys(file: &str, content: &str, findings: &mut Vec<Finding>) {
    // Check for duplicate top-level keys (common HA config error)
    let mut seen_keys: HashMap<String, usize> = HashMap::new();

    for (line_num, line) in content.lines().enumerate() {
        // Top-level key: starts at column 0, ends with ':'
        if !line.starts_with(' ') && !line.starts_with('#') && !line.trim().is_empty()
            && let Some(key) = line.split(':').next() {
                let key = key.trim();
                if !key.is_empty() && !key.starts_with('-') {
                    if let Some(prev_line) = seen_keys.get(key) {
                        findings.push(Finding {
                            file: file.to_string(),
                            line: Some(line_num + 1),
                            severity: "error",
                            check: "duplicate_key",
                            message: format!(
                                "Duplicate top-level key '{key}' (first seen at line {prev_line})"
                            ),
                        });
                    } else {
                        seen_keys.insert(key.to_string(), line_num + 1);
                    }
                }
            }
    }
}

fn check_jinja_templates(file: &str, content: &str, findings: &mut Vec<Finding>) {
    // HA YAML uses multi-line scalars (> and |) where Jinja delimiters span lines.
    // Check balance across entire file, not per-line.

    // First check statement balance ({%...%})
    let open_stmt = content.matches("{%").count();
    let close_stmt = content.matches("%}").count();
    if open_stmt != close_stmt {
        findings.push(Finding {
            file: file.to_string(),
            line: None,
            severity: "error",
            check: "jinja_balance",
            message: format!(
                "Unbalanced Jinja statement delimiters in file: {open_stmt} '{{%' vs {close_stmt} '%}}'"
            ),
        });
    }

    // For expression delimiters ({{/}}), strip content inside {%...%} blocks first
    // to avoid false positives from Python dict literals like {{"key": "val"}}
    let stmt_re = Regex::new(r"(?s)\{%.*?%\}").expect("valid regex");
    let stripped = stmt_re.replace_all(content, "");

    let open_expr = stripped.matches("{{").count();
    let close_expr = stripped.matches("}}").count();
    if open_expr != close_expr {
        findings.push(Finding {
            file: file.to_string(),
            line: None,
            severity: "error",
            check: "jinja_balance",
            message: format!(
                "Unbalanced Jinja expression delimiters in file: {open_expr} '{{{{' vs {close_expr} '}}}}'"
            ),
        });
    }
}

fn check_common_errors(file: &str, content: &str, findings: &mut Vec<Finding>) {
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Check for bare 'unavailable' return in template sensors (should use None)
        if trimmed == "unavailable" || trimmed == "'unavailable'" {
            // Look back a few lines for state_class or unit_of_measurement context
            let context_start = line_num.saturating_sub(5);
            let context = content
                .lines()
                .skip(context_start)
                .take(line_num - context_start)
                .any(|l| {
                    l.contains("unit_of_measurement")
                        || l.contains("state_class")
                        || l.contains("value_template")
                });
            if context {
                findings.push(Finding {
                    file: file.to_string(),
                    line: Some(line_num + 1),
                    severity: "warning",
                    check: "bare_unavailable",
                    message: "Template sensor returns bare 'unavailable' — use {{ none }} instead"
                        .to_string(),
                });
            }
        }

        // Check for None in numeric_state trigger thresholds
        if (trimmed.starts_with("above:") || trimmed.starts_with("below:"))
            && trimmed.contains("None")
        {
            findings.push(Finding {
                file: file.to_string(),
                line: Some(line_num + 1),
                severity: "error",
                check: "none_threshold",
                message: "numeric_state trigger with None threshold — use a number or remove the key"
                    .to_string(),
            });
        }

        // Check for tabs (YAML doesn't allow tabs for indentation)
        if line.contains('\t') && !trimmed.starts_with('#') {
            findings.push(Finding {
                file: file.to_string(),
                line: Some(line_num + 1),
                severity: "error",
                check: "tab_indent",
                message: "Tab character found — YAML requires spaces for indentation".to_string(),
            });
        }
    }
}

fn check_file_cruft(file: &str, findings: &mut Vec<Finding>) {
    let basename = Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let cruft_re = Regex::new(r"_v\d+\.|_old\.|_backup\.|_fixed\.|_improved\.|_optimized\.")
        .expect("valid regex");

    if cruft_re.is_match(basename) {
        findings.push(Finding {
            file: file.to_string(),
            line: None,
            severity: "warning",
            check: "file_cruft",
            message: "Versioned/backup file — consider archiving or removing".to_string(),
        });
    }
}

fn check_duplicate_entities_across_files(yaml_files: &[String], findings: &mut Vec<Finding>) {
    // Track input_* entity definitions across files
    let entity_re =
        Regex::new(r"(?m)^(input_number|input_boolean|input_datetime|input_select|input_text|counter|timer):\s*$")
            .expect("valid regex");
    let entity_name_re = Regex::new(r"^  ([a-z][a-z0-9_]+):\s*$").expect("valid regex");

    let mut entity_locations: HashMap<String, Vec<(String, usize)>> = HashMap::new();

    for file in yaml_files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut current_type: Option<String> = None;

        for (line_num, line) in content.lines().enumerate() {
            // Detect top-level entity type section
            if entity_re.is_match(line) {
                current_type = line.split(':').next().map(|s| s.trim().to_string());
                continue;
            }

            // Detect end of section (new top-level key)
            if !line.starts_with(' ') && !line.trim().is_empty() && !line.starts_with('#') {
                current_type = None;
                continue;
            }

            // Inside entity section, look for entity names
            if let Some(ref etype) = current_type
                && let Some(caps) = entity_name_re.captures(line) {
                    let name = caps.get(1).unwrap().as_str();
                    let full_id = format!("{etype}.{name}");
                    entity_locations
                        .entry(full_id)
                        .or_default()
                        .push((file.clone(), line_num + 1));
                }
        }
    }

    for (entity_id, locations) in &entity_locations {
        if locations.len() > 1 {
            let locs: Vec<String> = locations
                .iter()
                .map(|(f, l)| format!("{f}:{l}"))
                .collect();
            findings.push(Finding {
                file: locations[0].0.clone(),
                line: Some(locations[0].1),
                severity: "error",
                check: "duplicate_entity",
                message: format!("Duplicate entity '{entity_id}' defined in: {}", locs.join(", ")),
            });
        }
    }
}

async fn check_entity_references(
    client: &HaClient,
    yaml_files: &[String],
    findings: &mut Vec<Finding>,
) {
    // Extract entity references from states('...') calls
    let states_re = Regex::new(r"states\('([a-z_]+\.[a-z0-9_]+)'\)").expect("valid regex");
    let mut referenced: HashMap<String, (String, usize)> = HashMap::new();

    for file in yaml_files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_num, line) in content.lines().enumerate() {
            for cap in states_re.captures_iter(line) {
                let entity_id = cap.get(1).unwrap().as_str().to_string();
                referenced
                    .entry(entity_id)
                    .or_insert((file.clone(), line_num + 1));
            }
        }
    }

    if referenced.is_empty() {
        return;
    }

    // Batch check: get all states once
    let all_states: Vec<serde_json::Value> = match client.get_states().await {
        Ok(s) => s,
        Err(_) => return, // Can't check, skip silently
    };

    let existing: std::collections::HashSet<String> = all_states
        .iter()
        .filter_map(|s| s.get("entity_id").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();

    for (entity_id, (file, line)) in &referenced {
        if !existing.contains(entity_id) {
            findings.push(Finding {
                file: file.clone(),
                line: Some(*line),
                severity: "warning",
                check: "entity_ref",
                message: format!("Entity '{entity_id}' referenced but not found in HA"),
            });
        }
    }
}

fn check_sensor_platforms(file: &str, content: &str, findings: &mut Vec<Finding>) {
    let yaml: serde_yaml::Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(_) => return,
    };

    for domain in &["sensor", "binary_sensor"] {
        if let Some(serde_yaml::Value::Sequence(items)) = yaml.get(domain) {
            for (i, item) in items.iter().enumerate() {
                if let serde_yaml::Value::Mapping(map) = item {
                    let has_platform = map.contains_key(&serde_yaml::Value::String("platform".to_string()));
                    if !has_platform {
                        findings.push(Finding {
                            file: file.to_string(),
                            line: None,
                            severity: "warning",
                            check: "missing_platform",
                            message: format!(
                                "{domain} list entry #{} has no 'platform:' key — is this a legacy integration?",
                                i + 1
                            ),
                        });
                    }
                }
            }
        }
    }
}

fn check_package_exclusions(file: &str, content: &str, findings: &mut Vec<Finding>) {
    if !file.contains("/packages/") && !file.starts_with("packages/") {
        return;
    }

    const BLOCKED_KEYS: &[&str] = &[
        "homeassistant", "default_config", "frontend", "http",
        "recorder", "logger", "history", "logbook",
    ];

    for (line_num, line) in content.lines().enumerate() {
        if !line.starts_with(' ') && !line.starts_with('#') && !line.trim().is_empty() {
            if let Some(key) = line.split(':').next() {
                let key = key.trim();
                if BLOCKED_KEYS.contains(&key) {
                    findings.push(Finding {
                        file: file.to_string(),
                        line: Some(line_num + 1),
                        severity: "error",
                        check: "package_exclusion",
                        message: format!(
                            "Package contains '{key}:' which overrides main config — move to configuration.yaml"
                        ),
                    });
                }
            }
        }
    }
}

fn check_automation_syntax(file: &str, content: &str, findings: &mut Vec<Finding>) {
    let yaml: serde_yaml::Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let automations = match yaml.get("automation") {
        Some(serde_yaml::Value::Sequence(items)) => items,
        _ => return,
    };

    for (i, item) in automations.iter().enumerate() {
        let map = match item {
            serde_yaml::Value::Mapping(m) => m,
            _ => continue,
        };

        let has = |key: &str| map.contains_key(&serde_yaml::Value::String(key.to_string()));

        let alias = map
            .get(&serde_yaml::Value::String("alias".to_string()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("#{}", i + 1));

        let has_trigger = has("trigger") || has("triggers");
        let has_action = has("action") || has("actions");
        let has_alias = has("alias");

        if !has_trigger {
            findings.push(Finding {
                file: file.to_string(),
                line: None,
                severity: "error",
                check: "automation_syntax",
                message: format!("Automation '{alias}' missing trigger/triggers"),
            });
        }

        if !has_action {
            findings.push(Finding {
                file: file.to_string(),
                line: None,
                severity: "error",
                check: "automation_syntax",
                message: format!("Automation '{alias}' missing action/actions"),
            });
        }

        if !has_alias {
            findings.push(Finding {
                file: file.to_string(),
                line: None,
                severity: "warning",
                check: "automation_syntax",
                message: format!("Automation #{} has no alias — add one for readability", i + 1),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_unbalanced_jinja() {
        let mut findings = Vec::new();
        check_jinja_templates(
            "test.yaml",
            "value: {{ states('sensor.temp') }",
            &mut findings,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "jinja_balance");
    }

    #[test]
    fn balanced_multiline_jinja_no_findings() {
        let mut findings = Vec::new();
        // Multi-line Jinja template (common in HA YAML with > or | scalars)
        let content = "state: >\n  {{ is_state('light.kitchen', 'on') or\n     is_state('light.living', 'on') }}\n";
        check_jinja_templates("test.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn detect_tab_indent() {
        let mut findings = Vec::new();
        check_common_errors("test.yaml", "\tname: bad", &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "tab_indent");
    }

    #[test]
    fn detect_none_threshold() {
        let mut findings = Vec::new();
        check_common_errors("test.yaml", "      above: None", &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "none_threshold");
    }

    #[test]
    fn detect_duplicate_top_level_keys() {
        let mut findings = Vec::new();
        let content = "sensor:\n  - platform: template\nautomation:\n  - alias: test\nsensor:\n  - platform: rest\n";
        check_duplicate_keys("test.yaml", content, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "duplicate_key");
    }

    #[test]
    fn cruft_detection() {
        let mut findings = Vec::new();
        check_file_cruft("packages/hvac_v2.yaml", &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "file_cruft");
    }

    #[test]
    fn no_cruft_on_normal_file() {
        let mut findings = Vec::new();
        check_file_cruft("packages/hvac.yaml", &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn valid_yaml_no_findings() {
        let mut findings = Vec::new();
        check_yaml_syntax("test.yaml", "sensor:\n  - platform: template\n", &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn invalid_yaml_has_finding() {
        let mut findings = Vec::new();
        check_yaml_syntax("test.yaml", "sensor:\n  bad: [unclosed", &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "error");
    }

    #[test]
    fn package_exclusion_blocks_homeassistant_key() {
        let mut findings = Vec::new();
        let content = "homeassistant:\n  name: My Home\nsensor:\n  - platform: template\n";
        check_package_exclusions("packages/bad.yaml", content, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "error");
        assert_eq!(findings[0].check, "package_exclusion");
    }

    #[test]
    fn package_exclusion_allows_normal_keys() {
        let mut findings = Vec::new();
        let content = "sensor:\n  - platform: template\nautomation:\n  - alias: test\n";
        check_package_exclusions("packages/good.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn package_exclusion_blocks_recorder_key() {
        let mut findings = Vec::new();
        let content = "recorder:\n  purge_keep_days: 5\n";
        check_package_exclusions("packages/recorder.yaml", content, &mut findings);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn package_exclusion_skips_non_package_files() {
        let mut findings = Vec::new();
        let content = "homeassistant:\n  name: My Home\n";
        check_package_exclusions("configuration.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn package_exclusion_ignores_nested_key_names() {
        let mut findings = Vec::new();
        let content = "sensor:\n  - platform: template\n    sensors:\n      test:\n        value_template: \"{{ states('recorder.something') }}\"\n";
        check_package_exclusions("packages/tricky.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn package_exclusion_blocks_multiple_keys() {
        let mut findings = Vec::new();
        let content = "recorder:\n  purge_keep_days: 5\nlogger:\n  default: warning\n";
        check_package_exclusions("packages/monitoring.yaml", content, &mut findings);
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn sensor_platform_missing_detected() {
        let mut findings = Vec::new();
        let content = "sensor:\n  - name: My Sensor\n    state: '{{ 1 }}'\n";
        check_sensor_platforms("test.yaml", content, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "missing_platform");
    }

    #[test]
    fn sensor_platform_present_no_finding() {
        let mut findings = Vec::new();
        let content = "sensor:\n  - platform: template\n    sensors:\n      test:\n        value_template: '{{ 1 }}'\n";
        check_sensor_platforms("test.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn binary_sensor_platform_missing_detected() {
        let mut findings = Vec::new();
        let content = "binary_sensor:\n  - name: Door\n    state: 'on'\n";
        check_sensor_platforms("test.yaml", content, &mut findings);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn sensor_not_a_list_no_panic() {
        let mut findings = Vec::new();
        let content = "sensor: true\n";
        check_sensor_platforms("test.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn automation_syntax_valid_modern() {
        let mut findings = Vec::new();
        let content = "automation:\n  - alias: Test\n    triggers:\n      - trigger: state\n    actions:\n      - action: light.turn_on\n";
        check_automation_syntax("test.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn automation_syntax_valid_legacy() {
        let mut findings = Vec::new();
        let content = "automation:\n  - alias: Test\n    trigger:\n      - platform: state\n    action:\n      - service: light.turn_on\n";
        check_automation_syntax("test.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn automation_syntax_missing_trigger_errors() {
        let mut findings = Vec::new();
        let content = "automation:\n  - alias: Broken\n    action:\n      - service: light.turn_on\n";
        check_automation_syntax("test.yaml", content, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "error");
        assert!(findings[0].message.contains("trigger"));
    }

    #[test]
    fn automation_syntax_missing_action_errors() {
        let mut findings = Vec::new();
        let content = "automation:\n  - alias: Broken\n    trigger:\n      - platform: state\n";
        check_automation_syntax("test.yaml", content, &mut findings);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("action"));
    }

    #[test]
    fn automation_syntax_missing_alias_warns() {
        let mut findings = Vec::new();
        let content = "automation:\n  - trigger:\n      - platform: state\n    action:\n      - service: light.turn_on\n";
        check_automation_syntax("test.yaml", content, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "warning");
        assert!(findings[0].message.contains("alias"));
    }

    #[test]
    fn automation_not_a_list_no_panic() {
        let mut findings = Vec::new();
        let content = "automation: !include automations.yaml\n";
        check_automation_syntax("test.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }
}
