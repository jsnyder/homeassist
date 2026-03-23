use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Home Assistant URL not configured. Set HA_URL or use --url")]
    MissingUrl,

    #[error("Home Assistant token not configured. Set HA_TOKEN or use --token")]
    MissingToken,

    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Invalid JSON for --{option}: {message}")]
    JsonParse { option: String, message: String },

    #[error("Invalid service format: \"{input}\". Expected format: domain.service (e.g., light.turn_on)")]
    InvalidServiceFormat { input: String },

    #[error("Invalid component: \"{input}\". Valid options: automations, scripts, scenes, all")]
    InvalidReloadComponent { input: String },

    #[error("Invalid regex pattern: {0}")]
    InvalidPattern(String),

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
            AppError::MissingUrl | AppError::MissingToken => "AUTH_FAILED",
            AppError::Http { .. } | AppError::Connection(_) => "CONNECTION_FAILED",
            AppError::JsonParse { .. } => "JSON_PARSE_ERROR",
            AppError::InvalidServiceFormat { .. } => "INVALID_SERVICE",
            AppError::InvalidReloadComponent { .. } => "INVALID_COMPONENT",
            AppError::InvalidPattern(_) => "INVALID_PATTERN",
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
            AppError::Connection(err.to_string())
        } else if let Some(status) = err.status() {
            AppError::Http {
                status: status.as_u16(),
                message: err.to_string(),
            }
        } else {
            AppError::Other(err.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_correct() {
        assert_eq!(AppError::MissingUrl.code(), "AUTH_FAILED");
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
        assert!(json.contains("AUTH_FAILED"));
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
}
