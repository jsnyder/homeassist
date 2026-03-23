use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use crate::validation::safe_regex;
use serde_json::json;

pub async fn errors(
    client: &HaClient,
    tail: Option<usize>,
    pattern: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    let log = client.get_error_log().await?;
    let mut lines: Vec<&str> = log.lines().collect();

    if let Some(p) = pattern {
        let re = safe_regex(p, true)?;
        lines.retain(|line| re.is_match(line));
    }

    if let Some(n) = tail {
        let start = lines.len().saturating_sub(n);
        lines = lines[start..].to_vec();
    }

    if mode == OutputMode::Compact {
        Ok(lines.join("\n"))
    } else {
        format_output(
            &json!({
                "lines": lines.len(),
                "log": lines,
            }),
            mode,
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tail_limits_output() {
        let lines = vec!["a", "b", "c", "d", "e"];
        let n = 3usize;
        let start = lines.len().saturating_sub(n);
        let result = &lines[start..];
        assert_eq!(result, &["c", "d", "e"]);
    }

    #[test]
    fn tail_larger_than_input() {
        let lines = vec!["a", "b"];
        let n = 10usize;
        let start = lines.len().saturating_sub(n);
        let result = &lines[start..];
        assert_eq!(result, &["a", "b"]);
    }
}
