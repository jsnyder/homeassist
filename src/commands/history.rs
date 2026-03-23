use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use serde_json::json;

pub async fn get(
    client: &HaClient,
    entity_id: &str,
    hours: u32,
    mode: OutputMode,
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
            .collect();

        if numeric.is_empty() {
            // Non-numeric: show state transitions
            let transitions: Vec<String> = history
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
            Ok(transitions.join("\n"))
        } else {
            let min = numeric.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = numeric.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let avg = numeric.iter().sum::<f64>() / numeric.len() as f64;
            Ok(format!(
                "min={min:.1}\tmax={max:.1}\tavg={avg:.1}\tsamples={}",
                numeric.len()
            ))
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
    #[test]
    fn compact_numeric_summary() {
        // Simulate what the compact formatter would produce
        let numeric = vec![20.0, 25.0, 30.0, 22.5];
        let min = numeric.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = numeric.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = numeric.iter().sum::<f64>() / numeric.len() as f64;
        assert_eq!(min, 20.0);
        assert_eq!(max, 30.0);
        assert!((avg - 24.375).abs() < 0.01);
    }
}
