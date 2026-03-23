use crate::auth::AuthConfig;
use crate::error::AppError;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

pub struct HaClient {
    client: Client,
    base_url: String,
}

impl HaClient {
    pub fn new(auth: &AuthConfig) -> Result<Self, AppError> {
        let client = Client::builder()
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {}", auth.token)
                        .parse()
                        .map_err(|e| AppError::Other(format!("Invalid token: {e}")))?,
                );
                headers
            })
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::Other(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self {
            client,
            base_url: auth.url.clone(),
        })
    }

    /// Check response status and return an appropriate error for non-success responses.
    async fn check_response(&self, resp: reqwest::Response) -> Result<reqwest::Response, AppError> {
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Http {
                status: status.as_u16(),
                message: if text.is_empty() {
                    status.canonical_reason().unwrap_or("Request failed").to_string()
                } else {
                    text
                },
            });
        }
        Ok(resp)
    }

    async fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T, AppError> {
        let url = format!("{}/api{}", self.base_url, endpoint);
        let resp = self.client.get(&url).send().await?;
        let resp = self.check_response(resp).await?;
        resp.json().await.map_err(Into::into)
    }

    async fn post<T: DeserializeOwned>(&self, endpoint: &str, body: &Value) -> Result<T, AppError> {
        let url = format!("{}/api{}", self.base_url, endpoint);
        let resp = self.client.post(&url).json(body).send().await?;
        let resp = self.check_response(resp).await?;
        resp.json().await.map_err(Into::into)
    }

    async fn post_text(&self, endpoint: &str, body: &Value) -> Result<String, AppError> {
        let url = format!("{}/api{}", self.base_url, endpoint);
        let resp = self.client.post(&url).json(body).send().await?;
        let resp = self.check_response(resp).await?;
        resp.text().await.map_err(Into::into)
    }

    // --- Public API methods ---

    pub async fn get_config(&self) -> Result<Value, AppError> {
        self.get("/config").await
    }

    pub async fn get_states(&self) -> Result<Vec<Value>, AppError> {
        self.get("/states").await
    }

    pub async fn get_state(&self, entity_id: &str) -> Result<Value, AppError> {
        let encoded = encode_path_segment(entity_id);
        self.get(&format!("/states/{encoded}")).await
    }

    pub async fn get_services(&self) -> Result<Vec<Value>, AppError> {
        self.get("/services").await
    }

    pub async fn call_service(
        &self,
        domain: &str,
        service: &str,
        data: Value,
    ) -> Result<Value, AppError> {
        let domain = encode_path_segment(domain);
        let service = encode_path_segment(service);
        self.post(&format!("/services/{domain}/{service}"), &data)
            .await
    }

    pub async fn render_template(&self, template: &str) -> Result<String, AppError> {
        let body = serde_json::json!({ "template": template });
        self.post_text("/template", &body).await
    }

    pub async fn check_config(&self) -> Result<Value, AppError> {
        self.post("/config/core/check_config", &serde_json::json!({}))
            .await
    }

    pub async fn get_error_log(&self) -> Result<String, AppError> {
        let url = format!("{}/api/error_log", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let resp = self.check_response(resp).await?;
        resp.text().await.map_err(Into::into)
    }

    pub async fn get_history(
        &self,
        entity_id: &str,
        hours: u32,
    ) -> Result<Vec<Vec<Value>>, AppError> {
        let start = chrono_offset(hours)?;
        let url = format!("{}/api/history/period/{start}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("filter_entity_id", entity_id),
                ("minimal_response", ""),
            ])
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        resp.json().await.map_err(Into::into)
    }

    pub async fn get_logbook(
        &self,
        entity_id: &str,
        hours: u32,
    ) -> Result<Vec<Value>, AppError> {
        let start = chrono_offset(hours)?;
        let url = format!("{}/api/logbook/{start}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("entity", entity_id)])
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        resp.json().await.map_err(Into::into)
    }

    pub async fn fire_event(
        &self,
        event_type: &str,
        data: Value,
    ) -> Result<Value, AppError> {
        let encoded = encode_path_segment(event_type);
        self.post(&format!("/events/{encoded}"), &data).await
    }

    /// Transform services array into domain -> services map
    pub fn services_to_map(services: &[Value]) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        for item in services {
            if let (Some(domain), Some(svcs)) = (
                item.get("domain").and_then(|d| d.as_str()),
                item.get("services"),
            ) {
                map.insert(domain.to_string(), svcs.clone());
            }
        }
        map
    }
}

fn encode_path_segment(s: &str) -> String {
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

fn chrono_offset(hours: u32) -> Result<String, AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AppError::Other("System clock is before Unix epoch".into()))?
        .as_secs();
    let offset = now - (hours as u64 * 3600);
    let secs_per_day = 86400u64;
    let days_since_epoch = offset / secs_per_day;
    let secs_today = offset % secs_per_day;
    let hours_today = secs_today / 3600;
    let mins = (secs_today % 3600) / 60;
    let secs_rem = secs_today % 60;

    let (year, month, day) = days_to_ymd(days_since_epoch);
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hours_today:02}:{mins:02}:{secs_rem:02}"
    ))
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_to_map_transforms_correctly() {
        let services = vec![
            serde_json::json!({
                "domain": "light",
                "services": {
                    "turn_on": { "description": "Turn on light" },
                    "turn_off": { "description": "Turn off light" }
                }
            }),
            serde_json::json!({
                "domain": "switch",
                "services": {
                    "toggle": { "description": "Toggle switch" }
                }
            }),
        ];

        let map = HaClient::services_to_map(&services);
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("light"));
        assert!(map.contains_key("switch"));
        assert!(map["light"].get("turn_on").is_some());
    }

    #[test]
    fn services_to_map_empty_input() {
        let map = HaClient::services_to_map(&[]);
        assert!(map.is_empty());
    }

    #[test]
    fn chrono_offset_produces_valid_format() {
        let result = chrono_offset(24).unwrap();
        // Should look like YYYY-MM-DDTHH:MM:SS
        assert!(result.contains('T'));
        assert_eq!(result.len(), 19);
    }

    #[test]
    fn days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2024-01-01 is day 19723
        let (y, m, d) = days_to_ymd(19723);
        assert_eq!(y, 2024);
        assert_eq!(m, 1);
        assert_eq!(d, 1);
    }

    #[test]
    fn encode_path_segment_encodes_special_chars() {
        assert_eq!(encode_path_segment("light.kitchen"), "light%2Ekitchen");
        assert_eq!(encode_path_segment("sensor.temp"), "sensor%2Etemp");
    }

    #[test]
    fn encode_path_segment_preserves_alphanumeric() {
        assert_eq!(encode_path_segment("abc123"), "abc123");
    }
}
