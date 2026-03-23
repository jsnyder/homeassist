use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Home Assistant URL not configured. Set HA_URL or use --url")]
    MissingUrl,

    #[error("Home Assistant token not configured. Set HA_TOKEN or use --token")]
    MissingToken,

    #[error("{path} has insecure permissions ({mode:o}). Run: chmod 600 {path}")]
    InsecurePermissions { path: String, mode: u32 },

    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    #[error("Connection failed: unable to reach Home Assistant")]
    Connection(String),

    #[error("Invalid JSON for --{option}: {message}")]
    JsonParse { option: String, message: String },

    #[error("Invalid service format: \"{input}\". Expected format: domain.service (e.g., light.turn_on)")]
    InvalidServiceFormat { input: String },

    #[error("Invalid component: \"{input}\". Valid options: automations, scripts, scenes, all")]
    InvalidReloadComponent { input: String },

    #[error("Invalid regex pattern: {0}")]
    InvalidPattern(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("{0}")]
    Other(String),
}

#[derive(Serialize)]
pub struct ErrorOutput {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
}

impl AppError {
    pub fn code(&self) -> &str {
        match self {
            AppError::MissingUrl => "CONFIG_ERROR",
            AppError::MissingToken => "AUTH_FAILED",
            AppError::InsecurePermissions { .. } => "AUTH_FAILED",
            AppError::Http { .. } | AppError::Connection(_) => "CONNECTION_FAILED",
            AppError::JsonParse { .. } => "JSON_PARSE_ERROR",
            AppError::InvalidServiceFormat { .. } => "INVALID_SERVICE",
            AppError::InvalidReloadComponent { .. } => "INVALID_COMPONENT",
            AppError::InvalidPattern(_) => "INVALID_PATTERN",
            AppError::Serialization(_) => "SERIALIZATION_ERROR",
            AppError::Other(_) => "UNKNOWN_ERROR",
        }
    }

    pub fn to_error_output(&self) -> ErrorOutput {
        ErrorOutput {
            error: self.to_string(),
            code: self.code().to_string(),
            entity_id: None,
            service: None,
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_connect() {
            // Store raw error internally but display sanitized message
            AppError::Connection(err.to_string())
        } else if let Some(status) = err.status() {
            AppError::Http {
                status: status.as_u16(),
                message: status.canonical_reason().unwrap_or("Request failed").to_string(),
            }
        } else {
            AppError::Other(err.to_string())
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Serialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_correct() {
        assert_eq!(AppError::MissingUrl.code(), "CONFIG_ERROR");
        assert_eq!(AppError::MissingToken.code(), "AUTH_FAILED");
        assert_eq!(
            AppError::Http {
                status: 404,
                message: "not found".into()
            }
            .code(),
            "CONNECTION_FAILED"
        );
        assert_eq!(
            AppError::InvalidServiceFormat {
                input: "bad".into()
            }
            .code(),
            "INVALID_SERVICE"
        );
    }

    #[test]
    fn error_output_serializes_to_json() {
        let err = AppError::MissingUrl;
        let output = err.to_error_output();
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("CONFIG_ERROR"));
        assert!(json.contains("HA_URL"));
        // Optional fields should be absent
        assert!(!json.contains("entity_id"));
    }

    #[test]
    fn http_error_includes_status() {
        let err = AppError::Http {
            status: 401,
            message: "Unauthorized".into(),
        };
        assert_eq!(err.to_string(), "HTTP 401: Unauthorized");
    }

    #[test]
    fn connection_error_is_sanitized() {
        let err = AppError::Connection("reqwest::Error { kind: Connect, url: https://internal.host:8123 }".into());
        // Display output should NOT contain the raw reqwest details
        assert_eq!(err.to_string(), "Connection failed: unable to reach Home Assistant");
    }

    #[test]
    fn insecure_permissions_error() {
        let err = AppError::InsecurePermissions {
            path: "~/.ha_token".into(),
            mode: 0o644,
        };
        assert!(err.to_string().contains("chmod 600"));
        assert_eq!(err.code(), "AUTH_FAILED");
    }
}
