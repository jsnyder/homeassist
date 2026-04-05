use crate::client::HaClient;
use crate::error::AppError;
use crate::output::OutputMode;
use crate::ui;
use crate::ws::HaWebSocket;
use serde_json::{json, Value};

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

    match mode {
        OutputMode::Compact => Ok(format_compact(&stats)),
        OutputMode::Human => Ok(format_human(&stats)),
        OutputMode::Json => format_json(&stats),
    }
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
}
