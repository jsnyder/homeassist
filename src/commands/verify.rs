use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{OutputMode, format_output};
use crate::ui;
use serde_json::{Value, json};

pub async fn check(
    client: &HaClient,
    _url: &str,
    baseline_file: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    // Run all checks concurrently
    let human = mode == OutputMode::Human;
    let (config_result, states_result) = ui::with_spinner("Running checks\u{2026}", human, async {
        tokio::join!(client.get_config(), client.get_states(),)
    })
    .await;

    let config: Value = config_result?;
    let states: Vec<Value> = states_result?;

    let version = config
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let location = config
        .get("location_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Count entity states
    let total = states.len();
    let unavailable = states
        .iter()
        .filter(|e| e.get("state").and_then(|v| v.as_str()) == Some("unavailable"))
        .count();
    let unknown = states
        .iter()
        .filter(|e| e.get("state").and_then(|v| v.as_str()) == Some("unknown"))
        .count();

    // Domain breakdown
    let mut domain_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for entity in &states {
        if let Some(eid) = entity.get("entity_id").and_then(|v| v.as_str())
            && let Some(domain) = eid.split('.').next()
        {
            *domain_counts.entry(domain.to_string()).or_insert(0) += 1;
        }
    }

    // Automation states
    let automations: Vec<&Value> = states
        .iter()
        .filter(|e| {
            e.get("entity_id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id.starts_with("automation."))
        })
        .collect();
    let automations_on = automations
        .iter()
        .filter(|a| a.get("state").and_then(|v| v.as_str()) == Some("on"))
        .count();
    let automations_off = automations.len() - automations_on;

    // Check for HA config validity
    let config_valid = match client.check_config().await {
        Ok(result) => result.get("result").and_then(|v| v.as_str()) == Some("valid"),
        Err(_) => false,
    };

    // Load baseline if provided
    let baseline_delta = if let Some(bf) = baseline_file {
        load_and_compare_baseline(bf, unavailable, unknown)
    } else {
        None
    };

    // Save current snapshot for future baseline use
    let snapshot = json!({
        "unavailable": unavailable,
        "unknown": unknown,
        "total": total,
        "timestamp": current_timestamp(),
    });

    // Determine overall pass/fail
    let mut issues: Vec<String> = Vec::new();
    if !config_valid {
        issues.push("Configuration validation failed".to_string());
    }
    if let Some(ref delta) = baseline_delta
        && let Some(new_unavail) = delta.get("unavailable_delta").and_then(|v| v.as_i64())
        && new_unavail > 5
    {
        issues.push(format!(
            "Unavailable entities increased by {new_unavail} since baseline"
        ));
    }

    let passed = issues.is_empty();

    if mode == OutputMode::Compact {
        let mut lines = Vec::new();
        lines.push(format!(
            "status:{}\tversion:{}\tlocation:{}",
            if passed { "PASS" } else { "FAIL" },
            version,
            location
        ));
        lines.push(format!(
            "entities:{}\tunavailable:{}\tunknown:{}",
            total, unavailable, unknown
        ));
        lines.push(format!(
            "automations:{} on, {} off",
            automations_on, automations_off
        ));
        lines.push(format!(
            "config:{}",
            if config_valid { "valid" } else { "INVALID" }
        ));
        if let Some(ref delta) = baseline_delta {
            lines.push(format!(
                "baseline_delta:unavailable={},unknown={}",
                delta
                    .get("unavailable_delta")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                delta
                    .get("unknown_delta")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
            ));
        }
        if !issues.is_empty() {
            for issue in &issues {
                lines.push(format!("ISSUE: {issue}"));
            }
        }
        // Write snapshot to stderr for capture
        eprintln!("{}", serde_json::to_string(&snapshot).unwrap_or_default());
        Ok(lines.join("\n"))
    } else if mode == OutputMode::Human {
        let s = ui::Style::detect();
        let w = 14;
        let mut out = format!("{}\n\n", s.header("Deployment Verification"));

        // Config & version
        let config_display = if config_valid {
            format!("{}valid{}", s.green, s.reset)
        } else {
            format!("{}INVALID{}", s.red, s.reset)
        };
        out.push_str(&format!("{}\n", s.kv("Config", w, &config_display)));
        out.push_str(&format!("{}\n", s.kv("Version", w, version)));
        out.push_str(&format!("{}\n\n", s.kv("Location", w, location)));

        // Entities
        out.push_str(&format!(
            "{}\n",
            s.kv("Entities", w, &format!("{} total", ui::fmt_num(total)))
        ));
        if unavailable > 0 {
            out.push_str(&format!(
                "  {:<w$}  {}{} unavailable{}\n",
                "",
                ui::fmt_num(unavailable),
                s.reset,
                s.reset,
                w = w,
            ));
        }
        if unknown > 0 {
            out.push_str(&format!(
                "  {:<w$}  {}{} unknown{}\n",
                "",
                ui::fmt_num(unknown),
                s.reset,
                s.reset,
                w = w,
            ));
        }
        out.push('\n');

        // Automations
        out.push_str(&format!(
            "{}\n",
            s.kv(
                "Automations",
                w,
                &format!(
                    "{} on {}·{} {} off {}·{} {} total",
                    automations_on,
                    s.dim,
                    s.reset,
                    automations_off,
                    s.dim,
                    s.reset,
                    automations.len()
                )
            )
        ));

        // Baseline delta
        if let Some(ref delta) = baseline_delta {
            let ud = delta
                .get("unavailable_delta")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let uk = delta
                .get("unknown_delta")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let delta_str = format!("unavailable {:+}, unknown {:+}", ud, uk);
            out.push_str(&format!("\n{}\n", s.kv("Baseline", w, &delta_str)));
        }

        // Result
        out.push('\n');
        if passed {
            out.push_str(&format!("  {}\n", s.pass("All checks passed")));
        } else {
            out.push_str(&format!("  {}\n", s.fail("Issues detected")));
            for issue in &issues {
                out.push_str(&format!("    {} {}\n", s.fail(""), issue));
            }
        }

        Ok(out)
    } else {
        let mut result = json!({
            "passed": passed,
            "version": version,
            "location": location,
            "config_valid": config_valid,
            "entities": {
                "total": total,
                "unavailable": unavailable,
                "unknown": unknown,
            },
            "automations": {
                "on": automations_on,
                "off": automations_off,
                "total": automations.len(),
            },
            "snapshot": snapshot,
        });

        if !issues.is_empty() {
            result["issues"] = json!(issues);
        }
        if let Some(delta) = baseline_delta {
            result["baseline_delta"] = delta;
        }

        format_output(&result, mode)
    }
}

fn load_and_compare_baseline(
    path: &str,
    current_unavailable: usize,
    current_unknown: usize,
) -> Option<Value> {
    let content = std::fs::read_to_string(path).ok()?;
    let baseline: Value = serde_json::from_str(&content).ok()?;

    let prev_unavailable = baseline.get("unavailable").and_then(|v| v.as_i64())?;
    let prev_unknown = baseline.get("unknown").and_then(|v| v.as_i64())?;

    let current_unavailable = i64::try_from(current_unavailable).ok()?;
    let current_unknown = i64::try_from(current_unknown).ok()?;

    Some(json!({
        "unavailable_delta": current_unavailable - prev_unavailable,
        "unknown_delta": current_unknown - prev_unknown,
        "baseline_file": path,
    }))
}

fn current_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO-ish timestamp
    let secs_per_day = 86400u64;
    let days = now / secs_per_day;
    let secs_today = now % secs_per_day;
    let hours = secs_today / 3600;
    let mins = (secs_today % 3600) / 60;
    let secs = secs_today % 60;

    let (year, month, day) = crate::time::days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{mins:02}:{secs:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_comparison_works() {
        let tmp = std::env::temp_dir().join("ha_test_baseline.json");
        std::fs::write(&tmp, r#"{"unavailable":10,"unknown":5,"total":100}"#).unwrap();

        let delta = load_and_compare_baseline(tmp.to_str().unwrap(), 12, 3).unwrap();
        assert_eq!(delta["unavailable_delta"], 2);
        assert_eq!(delta["unknown_delta"], -2);

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn baseline_large_values_no_overflow() {
        let tmp = std::env::temp_dir().join("ha_test_baseline_large.json");
        let large = u64::MAX;
        std::fs::write(
            &tmp,
            format!(r#"{{"unavailable":{large},"unknown":0,"total":100}}"#),
        )
        .unwrap();

        // Should return None rather than wrapping/panicking
        let result = load_and_compare_baseline(tmp.to_str().unwrap(), 10, 0);
        assert!(
            result.is_none(),
            "should gracefully handle values exceeding i64 range"
        );

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn timestamp_format() {
        let ts = current_timestamp();
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
    }
}
