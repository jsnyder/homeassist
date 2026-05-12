use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{OutputMode, format_output};
use serde_json::json;

fn format_numeric_summary(values: &[f64]) -> String {
    if values.is_empty() {
        return "samples=0".to_string();
    }
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let avg = values.iter().sum::<f64>() / values.len() as f64;
    format!(
        "min={min:.1}\tmax={max:.1}\tavg={avg:.1}\tsamples={}",
        values.len()
    )
}

pub async fn get(
    client: &HaClient,
    entity_id: &str,
    hours: u32,
    mode: OutputMode,
    limit: Option<usize>,
) -> Result<String, AppError> {
    let history = client.get_history(entity_id, hours).await?;

    if mode == OutputMode::Compact {
        // Return min/max/avg summary for numeric states
        let states: Vec<&str> = history
            .iter()
            .flat_map(|series| series.iter())
            .filter_map(|entry| entry.get("state").and_then(|s| s.as_str()))
            .collect();

        let numeric: Vec<f64> = states
            .iter()
            .filter_map(|s| s.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .collect();
        let non_numeric = states.len() - numeric.len();

        if numeric.is_empty() {
            // Non-numeric: show state transitions
            let all_transitions: Vec<String> = history
                .iter()
                .flat_map(|series| series.iter())
                .filter_map(|entry| {
                    let state = entry.get("state")?.as_str()?;
                    let changed = entry
                        .get("last_changed")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    Some(format!("{changed}\t{state}"))
                })
                .collect();
            let total = all_transitions.len();
            let cap = limit.unwrap_or(total);
            let mut transitions: Vec<String> = all_transitions.into_iter().take(cap).collect();
            if total > cap {
                transitions.push(format!("[+{} more]", total - cap));
            }
            Ok(transitions.join("\n"))
        } else {
            let mut summary = format_numeric_summary(&numeric);
            if non_numeric > 0 {
                summary.push_str(&format!("\tnon_numeric:{non_numeric}"));
            }
            Ok(summary)
        }
    } else {
        format_output(
            &json!({
                "entity_id": entity_id,
                "hours": hours,
                "history": history,
            }),
            mode,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_numeric_summary_basic() {
        let result = format_numeric_summary(&[20.0, 25.0, 30.0, 22.5]);
        assert!(result.contains("min=20.0"));
        assert!(result.contains("max=30.0"));
        assert!(result.contains("avg=24.4"));
        assert!(result.contains("samples=4"));
    }

    #[test]
    fn format_numeric_summary_empty() {
        let result = format_numeric_summary(&[]);
        assert_eq!(result, "samples=0");
    }

    #[test]
    fn format_numeric_summary_single_value() {
        let result = format_numeric_summary(&[42.0]);
        assert!(result.contains("min=42.0"));
        assert!(result.contains("max=42.0"));
        assert!(result.contains("avg=42.0"));
        assert!(result.contains("samples=1"));
    }
}
