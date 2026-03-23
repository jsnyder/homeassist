use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use serde_json::{json, Value};
use std::time::Duration;

pub async fn entity(
    client: &HaClient,
    entity_id: &str,
    timeout_secs: u32,
    interval_secs: u32,
    target_state: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    let initial: Value = client.get_state(entity_id).await?;
    let initial_state = initial
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // If target_state given and already matches, return immediately
    if let Some(target) = target_state
        && initial_state == target
    {
        return format_output(
            &json!({
                "entity_id": entity_id,
                "state": initial_state,
                "matched": true,
                "elapsed_secs": 0,
            }),
            mode,
        );
    }

    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs as u64);
    let interval = Duration::from_secs(interval_secs as u64);

    loop {
        tokio::time::sleep(interval).await;

        if start.elapsed() > timeout {
            return format_output(
                &json!({
                    "entity_id": entity_id,
                    "state": initial_state,
                    "changed": false,
                    "timeout": true,
                    "elapsed_secs": start.elapsed().as_secs(),
                }),
                mode,
            );
        }

        let current: Value = client.get_state(entity_id).await?;
        let current_state = current
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if let Some(target) = target_state {
            if current_state == target {
                return format_output(
                    &json!({
                        "entity_id": entity_id,
                        "old_state": initial_state,
                        "state": current_state,
                        "matched": true,
                        "elapsed_secs": start.elapsed().as_secs(),
                    }),
                    mode,
                );
            }
        } else if current_state != initial_state {
            return format_output(
                &json!({
                    "entity_id": entity_id,
                    "old_state": initial_state,
                    "state": current_state,
                    "changed": true,
                    "elapsed_secs": start.elapsed().as_secs(),
                }),
                mode,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn timeout_duration_calculation() {
        let timeout = std::time::Duration::from_secs(300);
        assert_eq!(timeout.as_secs(), 300);
    }
}
