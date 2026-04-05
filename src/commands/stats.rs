use crate::client::HaClient;
use crate::error::AppError;
use crate::output::OutputMode;
use crate::ui;
use crate::ws::HaWebSocket;
use serde_json::{json, Value};
use std::collections::HashMap;

// ── Dashboard config structs ────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct DashboardConfig {
    sections: Vec<DashboardSection>,
}

#[derive(Debug, serde::Deserialize)]
struct DashboardSection {
    name: String,
    entities: Option<Vec<String>>,
    templates: Option<Vec<DashboardTemplate>>,
}

#[derive(Debug, serde::Deserialize)]
struct DashboardTemplate {
    label: String,
    template: String,
}

fn load_dashboard_config(path: Option<&str>) -> Option<DashboardConfig> {
    let candidates = if let Some(p) = path {
        vec![p.to_string()]
    } else {
        let home = std::env::var("HOME").unwrap_or_default();
        vec![
            ".homeassist-dashboard.yaml".to_string(),
            format!("{home}/.config/homeassist/dashboard.yaml"),
        ]
    };
    for candidate in candidates {
        if let Ok(content) = std::fs::read_to_string(&candidate) {
            return serde_yaml::from_str(&content).ok();
        }
    }
    None
}

// ── Section formatting (standalone, testable) ───────────────────────

fn format_section_compact(
    name: &str,
    entities: &[(&str, &str, Option<&str>)],
    templates: &[(&str, &str)],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("[{name}]"));
    for (id, state, unit) in entities {
        let unit_str = unit.unwrap_or("");
        parts.push(format!("{id}={state}{unit_str}"));
    }
    for (label, value) in templates {
        let label_clean = label.replace(' ', "_");
        parts.push(format!("{label_clean}={value}"));
    }
    parts.join("\t")
}

fn format_section_human(
    name: &str,
    entities: &[(&str, &str, Option<&str>)],
    templates: &[(&str, &str)],
) -> String {
    let s = ui::Style::detect();
    let mut out = format!("\n{}\n\n", s.header(name));
    for (id, state, unit) in entities {
        let (color, reset) = ui::state_color(state, &s);
        let unit_str = unit.map(|u| format!("  {}{u}{}", s.dim, s.reset)).unwrap_or_default();
        out.push_str(&format!(
            "  {:<35} {color}{:<12}{reset}{unit_str}\n",
            id, state,
        ));
    }
    for (label, value) in templates {
        out.push_str(&format!("  {:<35} {}\n", label, value));
    }
    out
}

fn format_section_json(
    name: &str,
    entities: &[(&str, &str, Option<&str>)],
    templates: &[(&str, &str)],
) -> Value {
    let entity_arr: Vec<Value> = entities
        .iter()
        .map(|(id, state, unit)| {
            json!({
                "entity_id": id,
                "state": state,
                "unit": unit.unwrap_or(""),
            })
        })
        .collect();
    let template_arr: Vec<Value> = templates
        .iter()
        .map(|(label, value)| json!({ "label": label, "value": value }))
        .collect();
    json!({
        "name": name,
        "entities": entity_arr,
        "templates": template_arr,
    })
}

#[derive(Debug)]
struct SystemStats {
    version: String,
    location: String,
    total_entities: usize,
    unavailable: usize,
    unknown: usize,
    automations_on: usize,
    automations_off: usize,
    scripts: usize,
    components: usize,
    error_count: Option<usize>,
    warning_count: Option<usize>,
}

fn format_compact(stats: &SystemStats) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "sys\tv{}\tconnected\tcomp:{}",
        stats.version, stats.components
    ));
    lines.push(format!(
        "ent\ttotal:{}\tunavail:{}\tunk:{}\tauto:{}/{}\tscripts:{}",
        stats.total_entities,
        stats.unavailable,
        stats.unknown,
        stats.automations_on,
        stats.automations_off,
        stats.scripts,
    ));
    if let (Some(errors), Some(warnings)) = (stats.error_count, stats.warning_count) {
        lines.push(format!("log\terrors:{}\twarn:{}", errors, warnings));
    }
    lines.join("\n")
}

fn format_human(stats: &SystemStats) -> String {
    let s = ui::Style::detect();
    let w = 15;
    let mut out = format!("{}\n\n", s.header("Home Assistant Status"));

    // Two-column layout
    let rows: Vec<(&str, String, &str, String)> = vec![
        (
            "Version",
            stats.version.clone(),
            "Entities",
            ui::fmt_num(stats.total_entities),
        ),
        (
            "Location",
            stats.location.clone(),
            "Unavailable",
            ui::fmt_num(stats.unavailable),
        ),
        (
            "Status",
            format!("{}connected{}", s.green, s.reset),
            "Unknown",
            ui::fmt_num(stats.unknown),
        ),
        (
            "Automations",
            format!("{} on / {} off", stats.automations_on, stats.automations_off),
            "Scripts",
            ui::fmt_num(stats.scripts),
        ),
        (
            "Components",
            ui::fmt_num(stats.components),
            "Errors",
            match (stats.error_count, stats.warning_count) {
                (Some(e), Some(w)) => format!("{} errors, {} warnings", e, w),
                _ => "n/a".to_string(),
            },
        ),
    ];

    for (lk, lv, rk, rv) in &rows {
        out.push_str(&format!(
            "  {}{:<w$}{}  {:<17}  {}{:<w$}{}  {}\n",
            s.dim, lk, s.reset, lv, s.dim, rk, s.reset, rv,
            w = w,
        ));
    }

    out
}

fn format_json(stats: &SystemStats) -> Result<String, AppError> {
    let mut obj = json!({
        "version": stats.version,
        "location": stats.location,
        "status": "connected",
        "total_entities": stats.total_entities,
        "unavailable": stats.unavailable,
        "unknown": stats.unknown,
        "automations_on": stats.automations_on,
        "automations_off": stats.automations_off,
        "scripts": stats.scripts,
        "components": stats.components,
    });
    if let Some(e) = stats.error_count {
        obj["errors"] = json!(e);
    }
    if let Some(w) = stats.warning_count {
        obj["warnings"] = json!(w);
    }
    Ok(serde_json::to_string_pretty(&obj)?)
}

async fn fetch_log_counts(base_url: &str, token: &str) -> Result<(usize, usize), AppError> {
    let mut ws = HaWebSocket::connect(base_url, token).await?;
    let result = ws.command("system_log/list").await;
    ws.close().await;
    let entries = result?;
    let arr = entries
        .as_array()
        .ok_or_else(|| AppError::Other("Expected array from system_log/list".into()))?;
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for entry in arr {
        match entry.get("level").and_then(|v| v.as_str()) {
            Some("ERROR") | Some("CRITICAL") => errors += 1,
            Some("WARNING") => warnings += 1,
            _ => {}
        }
    }
    Ok((errors, warnings))
}

pub async fn status(
    client: &HaClient,
    base_url: &str,
    token: &str,
    dashboard: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    let human = mode == OutputMode::Human;

    let (config, states) = ui::with_spinner(
        "Loading system status\u{2026}",
        human,
        async {
            let (c, s) = tokio::join!(client.get_config(), client.get_states());
            Ok::<(Value, Vec<Value>), AppError>((c?, s?))
        },
    )
    .await?;

    let version = config
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let location = config
        .get("location_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let components = config
        .get("components")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let mut total_entities = 0usize;
    let mut unavailable = 0usize;
    let mut unknown = 0usize;
    let mut automations_on = 0usize;
    let mut automations_off = 0usize;
    let mut scripts = 0usize;

    for entity in &states {
        total_entities += 1;
        let entity_id = entity
            .get("entity_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let state = entity
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match state {
            "unavailable" => unavailable += 1,
            "unknown" => unknown += 1,
            _ => {}
        }

        if entity_id.starts_with("automation.") {
            match state {
                "on" => automations_on += 1,
                "off" => automations_off += 1,
                _ => {}
            }
        } else if entity_id.starts_with("script.") {
            scripts += 1;
        }
    }

    // Try to get log counts via WebSocket (graceful fallback)
    let (error_count, warning_count) = match fetch_log_counts(base_url, token).await {
        Ok((e, w)) => (Some(e), Some(w)),
        Err(_) => (None, None),
    };

    let stats = SystemStats {
        version,
        location,
        total_entities,
        unavailable,
        unknown,
        automations_on,
        automations_off,
        scripts,
        components,
        error_count,
        warning_count,
    };

    let mut result = match mode {
        OutputMode::Compact => format_compact(&stats),
        OutputMode::Human => format_human(&stats),
        OutputMode::Json => format_json(&stats)?,
    };

    // Dashboard sections
    if let Some(config) = load_dashboard_config(dashboard) {
        // Build entity lookup map
        let state_map: HashMap<&str, &Value> = states
            .iter()
            .filter_map(|e| {
                let id = e.get("entity_id")?.as_str()?;
                Some((id, e))
            })
            .collect();

        let mut json_sections: Vec<Value> = Vec::new();

        for section in &config.sections {
            // Resolve entities
            let resolved_entities: Vec<(&str, String, Option<String>)> = section
                .entities
                .as_ref()
                .map(|ents| {
                    ents.iter()
                        .map(|eid| {
                            if let Some(entity) = state_map.get(eid.as_str()) {
                                let st = entity
                                    .get("state")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let unit = entity
                                    .get("attributes")
                                    .and_then(|a| a.get("unit_of_measurement"))
                                    .and_then(|u| u.as_str())
                                    .map(|s| s.to_string());
                                (eid.as_str(), st, unit)
                            } else {
                                (eid.as_str(), "not_found".to_string(), None)
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Resolve templates in parallel
            let resolved_templates: Vec<(String, String)> = if let Some(tmpls) = &section.templates
            {
                let futs: Vec<_> = tmpls
                    .iter()
                    .map(|t| {
                        let label = t.label.clone();
                        let tmpl = t.template.clone();
                        async move {
                            let val = client
                                .render_template(&tmpl)
                                .await
                                .unwrap_or_else(|e| format!("error: {e}"));
                            (label, val.trim().to_string())
                        }
                    })
                    .collect();
                futures_util::future::join_all(futs).await
            } else {
                Vec::new()
            };

            // Convert to borrowed slices for formatting functions
            let ent_refs: Vec<(&str, &str, Option<&str>)> = resolved_entities
                .iter()
                .map(|(id, st, unit)| (*id, st.as_str(), unit.as_deref()))
                .collect();
            let tmpl_refs: Vec<(&str, &str)> = resolved_templates
                .iter()
                .map(|(l, v)| (l.as_str(), v.as_str()))
                .collect();

            match mode {
                OutputMode::Compact => {
                    result.push('\n');
                    result.push_str(&format_section_compact(&section.name, &ent_refs, &tmpl_refs));
                }
                OutputMode::Human => {
                    result.push_str(&format_section_human(&section.name, &ent_refs, &tmpl_refs));
                }
                OutputMode::Json => {
                    json_sections.push(format_section_json(
                        &section.name,
                        &ent_refs,
                        &tmpl_refs,
                    ));
                }
            }
        }

        // For JSON mode, inject sections into the existing JSON object
        if mode == OutputMode::Json && !json_sections.is_empty() {
            let mut parsed: Value = serde_json::from_str(&result)?;
            parsed["sections"] = json!(json_sections);
            result = serde_json::to_string_pretty(&parsed)?;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_stats() -> SystemStats {
        SystemStats {
            version: "2024.12.1".to_string(),
            location: "Home".to_string(),
            total_entities: 847,
            unavailable: 3,
            unknown: 1,
            automations_on: 62,
            automations_off: 4,
            scripts: 23,
            components: 187,
            error_count: Some(12),
            warning_count: Some(9),
        }
    }

    #[test]
    fn compact_format_three_lines() {
        let output = format_compact(&sample_stats());
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("sys\t"));
        assert!(lines[0].contains("v2024.12.1"));
        assert!(lines[0].contains("comp:187"));
        assert!(lines[1].starts_with("ent\t"));
        assert!(lines[1].contains("total:847"));
        assert!(lines[1].contains("unavail:3"));
        assert!(lines[1].contains("auto:62/4"));
        assert!(lines[2].starts_with("log\t"));
        assert!(lines[2].contains("errors:12"));
    }

    #[test]
    fn compact_format_without_logs() {
        let mut stats = sample_stats();
        stats.error_count = None;
        stats.warning_count = None;
        let output = format_compact(&stats);
        assert_eq!(output.lines().count(), 2);
        assert!(!output.contains("log\t"));
    }

    #[test]
    fn human_format_contains_key_info() {
        let output = format_human(&sample_stats());
        assert!(output.contains("2024.12.1"));
        assert!(output.contains("847"));
        assert!(output.contains("62 on / 4 off"));
        assert!(output.contains("Home Assistant Status"));
    }

    #[test]
    fn json_format_is_valid() {
        let output = format_json(&sample_stats()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["total_entities"], 847);
        assert_eq!(parsed["version"], "2024.12.1");
        assert_eq!(parsed["errors"], 12);
    }

    #[test]
    fn parse_dashboard_config() {
        let yaml = r#"
sections:
  - name: Climate
    entities:
      - climate.thermostat
      - sensor.outdoor_temperature
    templates:
      - label: "Heat index"
        template: "{{ states('sensor.heat_index') }}"
  - name: Security
    entities:
      - lock.front_door
"#;
        let config: DashboardConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.sections.len(), 2);
        assert_eq!(config.sections[0].name, "Climate");
        assert_eq!(config.sections[0].entities.as_ref().unwrap().len(), 2);
        assert_eq!(config.sections[0].templates.as_ref().unwrap().len(), 1);
        assert!(config.sections[1].templates.is_none());
    }

    #[test]
    fn render_section_compact_format() {
        let entities = vec![
            ("climate.thermostat", "heat", Some("°F")),
            ("sensor.outdoor_temp", "85.2", Some("°F")),
        ];
        let templates = vec![("Net cooling", "42.1W")];
        let output = format_section_compact("Climate", &entities, &templates);
        assert!(output.starts_with("[Climate]"));
        assert!(output.contains("climate.thermostat=heat°F"));
        assert!(output.contains("Net_cooling=42.1W"));
    }

    #[test]
    fn render_section_human_contains_entities() {
        let entities = vec![("climate.thermostat", "heat", Some("°F"))];
        let output = format_section_human("Climate", &entities, &[]);
        assert!(output.contains("Climate"));
        assert!(output.contains("climate.thermostat"));
        assert!(output.contains("heat"));
    }

    #[test]
    fn load_dashboard_config_returns_none_for_missing() {
        let result = load_dashboard_config(Some("/nonexistent/path.yaml"));
        assert!(result.is_none());
    }
}
