use crate::client::HaClient;
use crate::error::AppError;
use crate::output::{format_output, OutputMode};
use crate::validation::{parse_json_option, validate_service_format};
use serde_json::json;

pub async fn list(
    client: &HaClient,
    domain: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    let services = client.get_services().await?;
    let map = HaClient::services_to_map(&services);

    if mode == OutputMode::Compact {
        if let Some(d) = domain {
            if let Some(svcs) = map.get(d) {
                let lines: Vec<String> = svcs
                    .as_object()
                    .map(|obj| {
                        obj.iter()
                            .map(|(name, svc)| {
                                let fields = svc
                                    .get("fields")
                                    .and_then(|f| f.as_object())
                                    .map(|f| {
                                        f.keys()
                                            .cloned()
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    })
                                    .unwrap_or_default();
                                format!("{d}.{name}({fields})")
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(lines.join("\n"))
            } else {
                Ok(String::new())
            }
        } else {
            let mut domains: Vec<&String> = map.keys().collect();
            domains.sort();
            Ok(domains.iter().map(|d| d.as_str()).collect::<Vec<_>>().join("\n"))
        }
    } else if let Some(d) = domain {
        if let Some(svcs) = map.get(d) {
            format_output(&json!({ d: svcs }), mode)
        } else {
            format_output(&json!({}), mode)
        }
    } else {
        format_output(&json!(map), mode)
    }
}

pub async fn call(
    client: &HaClient,
    service: &str,
    data_json: Option<&str>,
    target_json: Option<&str>,
    mode: OutputMode,
) -> Result<String, AppError> {
    let (domain, service_name) = validate_service_format(service)?;

    let mut data = if let Some(d) = data_json {
        parse_json_option(d, "data")?
    } else {
        json!({})
    };

    // Merge target into data
    if let Some(t) = target_json {
        let target = parse_json_option(t, "target")?;
        if let (Some(data_obj), Some(target_obj)) = (data.as_object_mut(), target.as_object()) {
            for (k, v) in target_obj {
                data_obj.insert(k.clone(), v.clone());
            }
        }
    }

    let result = client.call_service(domain, service_name, data).await?;
    format_output(&json!({ "success": true, "result": result }), mode)
}
