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

        // YAML syntax check (parse once, reuse for structural checks)
        match serde_yaml::from_str::<serde_yaml::Value>(&content) {
            Ok(yaml) => {
                check_duplicate_keys(file_path, &content, &mut findings);
                check_jinja_templates(file_path, &content, &mut findings);
                check_common_errors(file_path, &content, &mut findings);
                check_file_cruft(file_path, &mut findings);
                check_package_exclusions(file_path, &content, &mut findings);
                check_sensor_platforms(file_path, &yaml, &mut findings);
                check_automation_syntax(file_path, &yaml, &mut findings);
                detect_circular_references(file_path, &yaml, &mut findings);
            }
            Err(e) => {
                let line = e.location().map(|l| l.line());
                findings.push(Finding {
                    file: file_path.to_string(),
                    line,
                    severity: "error",
                    check: "yaml_syntax",
                    message: format!("Invalid YAML: {e}"),
                });
            }
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

fn check_sensor_platforms(file: &str, yaml: &serde_yaml::Value, findings: &mut Vec<Finding>) {
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
        "default_config", "frontend", "http",
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

fn check_automation_syntax(file: &str, yaml: &serde_yaml::Value, findings: &mut Vec<Finding>) {
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

        // Blueprint-based automations inherit trigger/action from the blueprint
        if has("use_blueprint") {
            continue;
        }

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

fn detect_circular_references(file: &str, yaml: &serde_yaml::Value, findings: &mut Vec<Finding>) {
    let template = match yaml.get("template") {
        Some(serde_yaml::Value::Sequence(items)) => items,
        _ => return,
    };

    for item in template {
        let map = match item.as_mapping() {
            Some(m) => m,
            None => continue,
        };

        for sensor_type in &["sensor", "binary_sensor"] {
            let sensors = match map.get(&serde_yaml::Value::String(sensor_type.to_string())) {
                Some(serde_yaml::Value::Sequence(s)) => s,
                _ => continue,
            };

            for sensor in sensors {
                let sensor_map = match sensor.as_mapping() {
                    Some(m) => m,
                    None => continue,
                };

                let name = sensor_map
                    .get(&serde_yaml::Value::String("name".into()))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unnamed");

                let unique_id = sensor_map
                    .get(&serde_yaml::Value::String("unique_id".into()))
                    .and_then(|v| v.as_str());

                let entity_id = if let Some(uid) = unique_id {
                    format!("{sensor_type}.{uid}")
                } else {
                    format!(
                        "{sensor_type}.{}",
                        name.to_lowercase().replace(' ', "_")
                    )
                };

                // Check state, icon, availability fields
                // icon is excluded: icon templates commonly reference their own state
                // to pick different icons, which is intentional and not circular
                for field in &["state", "availability"] {
                    if let Some(val) = sensor_map
                        .get(&serde_yaml::Value::String(field.to_string()))
                        .and_then(|v| v.as_str())
                    {
                        if contains_self_reference(val, &entity_id) {
                            findings.push(Finding {
                                file: file.to_string(),
                                line: None,
                                severity: "error",
                                check: "circular_reference",
                                message: format!(
                                    "Template sensor '{name}' ({entity_id}): {field} references itself"
                                ),
                            });
                        }
                    }
                }

                // Check attributes
                if let Some(attrs) = sensor_map
                    .get(&serde_yaml::Value::String("attributes".into()))
                    .and_then(|v| v.as_mapping())
                {
                    for (attr_key, attr_val) in attrs {
                        let attr_name = attr_key.as_str().unwrap_or("?");
                        if let Some(val) = attr_val.as_str() {
                            if contains_self_reference(val, &entity_id) {
                                findings.push(Finding {
                                    file: file.to_string(),
                                    line: None,
                                    severity: "error",
                                    check: "circular_reference",
                                    message: format!(
                                        "Template sensor '{name}' ({entity_id}): attribute '{attr_name}' references itself"
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn contains_self_reference(template: &str, entity_id: &str) -> bool {
    let escaped = regex::escape(entity_id);
    let re = Regex::new(&format!(
        r#"(states|state_attr|is_state)\(\s*['"]{}['")\s,]"#, escaped
    )).expect("valid regex");
    re.is_match(template)
}

fn find_orphaned_registry_entries(
    registry: &[serde_json::Value],
    states: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let state_ids: std::collections::HashSet<&str> = states
        .iter()
        .filter_map(|s| s.get("entity_id").and_then(|v| v.as_str()))
        .collect();

    registry
        .iter()
        .filter(|entry| {
            let eid = entry
                .get("entity_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let disabled = entry
                .get("disabled_by")
                .and_then(|v| v.as_str())
                .is_some();
            !eid.is_empty() && !disabled && !state_ids.contains(eid)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_yaml(content: &str) -> serde_yaml::Value {
        serde_yaml::from_str(content).unwrap()
    }

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
    fn valid_yaml_parses() {
        let result = serde_yaml::from_str::<serde_yaml::Value>("sensor:\n  - platform: template\n");
        assert!(result.is_ok());
    }

    #[test]
    fn invalid_yaml_fails_parse() {
        let result = serde_yaml::from_str::<serde_yaml::Value>("sensor:\n  bad: [unclosed");
        assert!(result.is_err());
    }

    #[test]
    fn package_exclusion_blocks_default_config_key() {
        let mut findings = Vec::new();
        let content = "default_config:\nsensor:\n  - platform: template\n";
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
        let content = "recorder:\n  purge_keep_days: 5\n";
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
        check_sensor_platforms("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "missing_platform");
    }

    #[test]
    fn sensor_platform_present_no_finding() {
        let mut findings = Vec::new();
        let content = "sensor:\n  - platform: template\n    sensors:\n      test:\n        value_template: '{{ 1 }}'\n";
        check_sensor_platforms("test.yaml", &parse_yaml(content), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn binary_sensor_platform_missing_detected() {
        let mut findings = Vec::new();
        let content = "binary_sensor:\n  - name: Door\n    state: 'on'\n";
        check_sensor_platforms("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn sensor_not_a_list_no_panic() {
        let mut findings = Vec::new();
        let content = "sensor: true\n";
        check_sensor_platforms("test.yaml", &parse_yaml(content), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn automation_syntax_valid_modern() {
        let mut findings = Vec::new();
        let content = "automation:\n  - alias: Test\n    triggers:\n      - trigger: state\n    actions:\n      - action: light.turn_on\n";
        check_automation_syntax("test.yaml", &parse_yaml(content), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn automation_syntax_valid_legacy() {
        let mut findings = Vec::new();
        let content = "automation:\n  - alias: Test\n    trigger:\n      - platform: state\n    action:\n      - service: light.turn_on\n";
        check_automation_syntax("test.yaml", &parse_yaml(content), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn automation_syntax_missing_trigger_errors() {
        let mut findings = Vec::new();
        let content = "automation:\n  - alias: Broken\n    action:\n      - service: light.turn_on\n";
        check_automation_syntax("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "error");
        assert!(findings[0].message.contains("trigger"));
    }

    #[test]
    fn automation_syntax_missing_action_errors() {
        let mut findings = Vec::new();
        let content = "automation:\n  - alias: Broken\n    trigger:\n      - platform: state\n";
        check_automation_syntax("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("action"));
    }

    #[test]
    fn automation_syntax_missing_alias_warns() {
        let mut findings = Vec::new();
        let content = "automation:\n  - trigger:\n      - platform: state\n    action:\n      - service: light.turn_on\n";
        check_automation_syntax("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "warning");
        assert!(findings[0].message.contains("alias"));
    }

    #[test]
    fn automation_not_a_list_no_panic() {
        let mut findings = Vec::new();
        let content = "automation: !include automations.yaml\n";
        check_automation_syntax("test.yaml", &parse_yaml(content), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn automation_blueprint_skipped() {
        let mut findings = Vec::new();
        let content = "automation:\n  - alias: Blueprint Auto\n    use_blueprint:\n      path: my_blueprint.yaml\n      input:\n        some_input: value\n";
        check_automation_syntax("test.yaml", &parse_yaml(content), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn package_exclusion_allows_homeassistant_key() {
        let mut findings = Vec::new();
        let content = "homeassistant:\n  customize:\n    sensor.temp:\n      friendly_name: Temperature\n";
        check_package_exclusions("packages/customize.yaml", content, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn circular_ref_self_reference_detected() {
        let mut findings = Vec::new();
        let content = "template:\n  - sensor:\n      - name: Test Sensor\n        unique_id: test_sensor\n        state: \"{{ states('sensor.test_sensor') }}\"\n";
        detect_circular_references("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "circular_reference");
        assert!(findings[0].message.contains("test_sensor"));
    }

    #[test]
    fn circular_ref_no_self_reference_clean() {
        let mut findings = Vec::new();
        let content = "template:\n  - sensor:\n      - name: Average Temp\n        unique_id: avg_temp\n        state: \"{{ states('sensor.outdoor_temp') }}\"\n";
        detect_circular_references("test.yaml", &parse_yaml(content), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn circular_ref_state_attr_self_reference() {
        let mut findings = Vec::new();
        let content = "template:\n  - sensor:\n      - name: Power Monitor\n        unique_id: power_monitor\n        state: \"{{ state_attr('sensor.power_monitor', 'watts') }}\"\n";
        detect_circular_references("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn circular_ref_is_state_self_reference() {
        let mut findings = Vec::new();
        let content = "template:\n  - binary_sensor:\n      - name: Door Open\n        unique_id: door_open\n        state: \"{{ is_state('binary_sensor.door_open', 'on') }}\"\n";
        detect_circular_references("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn circular_ref_name_derived_entity_id() {
        let mut findings = Vec::new();
        let content = "template:\n  - sensor:\n      - name: My Sensor\n        state: \"{{ states('sensor.my_sensor') }}\"\n";
        detect_circular_references("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn circular_ref_attribute_self_reference() {
        let mut findings = Vec::new();
        let content = "template:\n  - sensor:\n      - name: Power\n        unique_id: power_calc\n        state: \"{{ 100 }}\"\n        attributes:\n          trend: \"{{ states('sensor.power_calc') }}\"\n";
        detect_circular_references("test.yaml", &parse_yaml(content), &mut findings);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("attribute"));
    }

    #[test]
    fn circular_ref_no_false_positive_on_similar_name() {
        let mut findings = Vec::new();
        let content = "template:\n  - sensor:\n      - name: Test Sensor\n        unique_id: test_sensor\n        state: \"{{ states('sensor.test_sensor_2') }}\"\n";
        detect_circular_references("test.yaml", &parse_yaml(content), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn circular_ref_no_template_section_clean() {
        let mut findings = Vec::new();
        let content = "sensor:\n  - platform: template\n    sensors:\n      test:\n        value_template: '{{ 1 }}'\n";
        detect_circular_references("test.yaml", &parse_yaml(content), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn entity_registry_detects_orphaned_entries() {
        let registry = vec![
            json!({"entity_id": "sensor.temp", "platform": "template"}),
            json!({"entity_id": "sensor.deleted", "platform": "mqtt"}),
        ];
        let states = vec![
            json!({"entity_id": "sensor.temp", "state": "72"}),
        ];
        let orphaned = find_orphaned_registry_entries(&registry, &states);
        assert_eq!(orphaned.len(), 1);
        assert_eq!(orphaned[0]["entity_id"], "sensor.deleted");
    }

    #[test]
    fn entity_registry_ignores_disabled_entries() {
        let registry = vec![
            json!({"entity_id": "sensor.temp", "platform": "template"}),
            json!({"entity_id": "sensor.disabled", "platform": "mqtt", "disabled_by": "user"}),
        ];
        let states = vec![
            json!({"entity_id": "sensor.temp", "state": "72"}),
        ];
        let orphaned = find_orphaned_registry_entries(&registry, &states);
        assert!(orphaned.is_empty());
    }

    #[test]
    fn entity_registry_no_orphans_clean() {
        let registry = vec![
            json!({"entity_id": "sensor.temp", "platform": "template"}),
        ];
        let states = vec![
            json!({"entity_id": "sensor.temp", "state": "72"}),
        ];
        let orphaned = find_orphaned_registry_entries(&registry, &states);
        assert!(orphaned.is_empty());
    }
}
