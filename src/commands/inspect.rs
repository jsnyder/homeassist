use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{OutputMode, format_output};
use crate::ui;
use serde_json::{Value, json};

fn extract_domain(entity_id: &str) -> Option<&str> {
    entity_id.split('.').next().filter(|d| !d.is_empty())
}

pub async fn triage(
    client: &HaClient,
    mode: OutputMode,
    limit: Option<usize>,
) -> Result<String, AppError> {
    let human = mode == OutputMode::Human;
    let states: Vec<Value> =
        ui::with_spinner("Loading entities\u{2026}", human, client.get_states()).await?;

    let mut unavailable: Vec<Value> = Vec::new();
    let mut unknown: Vec<Value> = Vec::new();
    let mut domain_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for entity in &states {
        let entity_id = entity
            .get("entity_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let state = entity.get("state").and_then(|v| v.as_str()).unwrap_or("");

        if let Some(domain) = extract_domain(entity_id) {
            *domain_counts.entry(domain.to_string()).or_insert(0) += 1;
        }

        match state {
            "unavailable" => {
                unavailable.push(json!({
                    "entity_id": entity_id,
                    "friendly_name": entity.get("attributes")
                        .and_then(|a| a.get("friendly_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                }));
            }
            "unknown" => {
                unknown.push(json!({
                    "entity_id": entity_id,
                    "friendly_name": entity.get("attributes")
                        .and_then(|a| a.get("friendly_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                }));
            }
            _ => {}
        }
    }

    // Sort domain counts descending
    let mut domains: Vec<_> = domain_counts.into_iter().collect();
    domains.sort_by_key(|d| std::cmp::Reverse(d.1));

    format_triage_output(&unavailable, &unknown, states.len(), &domains, mode, limit)
}

fn format_triage_output(
    unavailable: &[Value],
    unknown: &[Value],
    total_entities: usize,
    domains: &[(String, usize)],
    mode: OutputMode,
    limit: Option<usize>,
) -> Result<String, AppError> {
    if mode == OutputMode::Compact {
        let mut lines = Vec::new();
        lines.push(format!(
            "total:{}\tunavailable:{}\tunknown:{}",
            total_entities,
            unavailable.len(),
            unknown.len()
        ));
        if !unavailable.is_empty() {
            let cap = limit.unwrap_or(unavailable.len());
            lines.push(format!("--- unavailable ({}) ---", unavailable.len()));
            for e in unavailable.iter().take(cap) {
                lines.push(format!(
                    "{}\t{}",
                    e["entity_id"].as_str().unwrap_or(""),
                    e["friendly_name"].as_str().unwrap_or("")
                ));
            }
            if unavailable.len() > cap {
                lines.push(format!("[+{} more]", unavailable.len() - cap));
            }
        }
        if !unknown.is_empty() {
            let cap = limit.unwrap_or(unknown.len());
            lines.push(format!("--- unknown ({}) ---", unknown.len()));
            for e in unknown.iter().take(cap) {
                lines.push(format!(
                    "{}\t{}",
                    e["entity_id"].as_str().unwrap_or(""),
                    e["friendly_name"].as_str().unwrap_or("")
                ));
            }
            if unknown.len() > cap {
                lines.push(format!("[+{} more]", unknown.len() - cap));
            }
        }
        Ok(lines.join("\n"))
    } else if mode == OutputMode::Human {
        let s = ui::Style::detect();
        let mut out = format!(
            "{}\n\n",
            s.header(&format!(
                "System Health \u{2014} {} entities",
                ui::fmt_num(total_entities)
            ))
        );

        let name_w = domains
            .iter()
            .map(|(k, _)| k.len())
            .max()
            .unwrap_or(10)
            .max(6);
        out.push_str(&format!(
            "  {}{:<name_w$}  {:>6}{}\n",
            s.dim, "Domain", "Count", s.reset,
        ));
        out.push_str(&format!("  {}\n", s.separator(name_w + 9)));
        for (domain, count) in domains {
            out.push_str(&format!(
                "  {:<name_w$}  {:>6}\n",
                domain,
                ui::fmt_num(*count),
            ));
        }

        out.push('\n');
        if !unavailable.is_empty() {
            out.push_str(&format!(
                "  {}{} unavailable{}\n",
                s.yellow,
                ui::fmt_num(unavailable.len()),
                s.reset,
            ));
        }
        if !unknown.is_empty() {
            out.push_str(&format!(
                "  {}{} unknown{}\n",
                s.yellow,
                ui::fmt_num(unknown.len()),
                s.reset,
            ));
        }
        if unavailable.is_empty() && unknown.is_empty() {
            out.push_str(&format!("  {}\n", s.pass("All entities healthy")));
        }

        Ok(out)
    } else {
        let cap = limit.unwrap_or(usize::MAX);
        let capped_unavail: Vec<&Value> = unavailable.iter().take(cap).collect();
        let capped_unknown: Vec<&Value> = unknown.iter().take(cap).collect();
        let domain_summary: Value = domains
            .iter()
            .map(|(k, v)| json!({"domain": k, "count": v}))
            .collect();

        format_output(
            &json!({
                "total_entities": total_entities,
                "unavailable_count": unavailable.len(),
                "unknown_count": unknown.len(),
                "unavailable": capped_unavail,
                "unknown": capped_unknown,
                "domains": domain_summary,
            }),
            mode,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_domain_from_entity_id() {
        assert_eq!(
            extract_domain("sensor.living_room_temperature"),
            Some("sensor")
        );
        assert_eq!(extract_domain("light.kitchen"), Some("light"));
    }

    #[test]
    fn format_triage_json_respects_limit() {
        let unavailable: Vec<Value> = (0..10)
            .map(|i| json!({"entity_id": format!("sensor.u{i}"), "friendly_name": format!("Sensor {i}")}))
            .collect();
        let unknown: Vec<Value> = (0..5)
            .map(|i| json!({"entity_id": format!("sensor.k{i}"), "friendly_name": format!("Unknown {i}")}))
            .collect();
        let output = super::format_triage_output(
            &unavailable,
            &unknown,
            100,
            &[],
            OutputMode::Json,
            Some(3),
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["unavailable"].as_array().unwrap().len(), 3);
        assert_eq!(parsed["unknown"].as_array().unwrap().len(), 3);
        assert_eq!(parsed["unavailable_count"], 10);
        assert_eq!(parsed["unknown_count"], 5);
    }

    #[test]
    fn extract_domain_edge_cases() {
        assert_eq!(extract_domain("no_dot"), Some("no_dot"));
        assert_eq!(extract_domain(""), None);
    }
}
