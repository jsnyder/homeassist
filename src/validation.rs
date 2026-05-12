use crate::error::AppError;
use regex::Regex;

const MAX_PATTERN_LENGTH: usize = 200;

/// Safely compile a user-provided regex pattern.
/// The `regex` crate guarantees O(n) matching — no ReDoS possible.
/// Pass `case_insensitive: true` to prepend `(?i)`.
pub fn safe_regex(pattern: &str, case_insensitive: bool) -> Result<Regex, AppError> {
    if pattern.is_empty() {
        return Err(AppError::InvalidPattern(
            "Pattern must be a non-empty string".into(),
        ));
    }
    if pattern.len() > MAX_PATTERN_LENGTH {
        return Err(AppError::InvalidPattern(format!(
            "Pattern too long (max {MAX_PATTERN_LENGTH} bytes)"
        )));
    }
    let pat = if case_insensitive {
        format!("(?i){pattern}")
    } else {
        pattern.to_string()
    };
    Regex::new(&pat).map_err(|e| AppError::InvalidPattern(e.to_string()))
}

/// Validate a service name format (domain.service)
pub fn validate_service_format(service: &str) -> Result<(&str, &str), AppError> {
    if service.is_empty() {
        return Err(AppError::InvalidServiceFormat {
            input: service.into(),
        });
    }
    let parts: Vec<&str> = service.splitn(2, '.').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AppError::InvalidServiceFormat {
            input: service.into(),
        });
    }
    Ok((parts[0], parts[1]))
}

/// Validate reload component name
pub fn validate_reload_component(component: Option<&str>) -> Result<&str, AppError> {
    let target = component.unwrap_or("all");
    match target {
        "automations" | "scripts" | "scenes" | "all" => Ok(target),
        _ => Err(AppError::InvalidReloadComponent {
            input: target.into(),
        }),
    }
}

/// Parse a JSON string with helpful error messages
pub fn parse_json_option(value: &str, option_name: &str) -> Result<serde_json::Value, AppError> {
    if value.is_empty() {
        return Err(AppError::JsonParse {
            option: option_name.into(),
            message: "requires a JSON string".into(),
        });
    }
    serde_json::from_str(value).map_err(|e| {
        if value.contains('\'') {
            AppError::JsonParse {
                option: option_name.into(),
                message: "Use double quotes, not single quotes".into(),
            }
        } else {
            AppError::JsonParse {
                option: option_name.into(),
                message: e.to_string(),
            }
        }
    })
}

pub fn parse_json_object_option(
    value: &str,
    option_name: &str,
) -> Result<serde_json::Value, AppError> {
    let val = parse_json_option(value, option_name)?;
    if !val.is_object() {
        return Err(AppError::JsonParse {
            option: option_name.into(),
            message: "must be a JSON object".into(),
        });
    }
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- safe_regex ---

    #[test]
    fn safe_regex_valid_pattern() {
        let re = safe_regex("kitchen", false).unwrap();
        assert!(re.is_match("light.kitchen"));
        assert!(!re.is_match("KITCHEN_sensor")); // case-sensitive by default
    }

    #[test]
    fn safe_regex_case_insensitive() {
        let re = safe_regex("Kitchen", true).unwrap();
        assert!(re.is_match("light.kitchen"));
        assert!(re.is_match("KITCHEN_sensor"));
    }

    #[test]
    fn safe_regex_case_sensitive() {
        let re = safe_regex("Kitchen", false).unwrap();
        assert!(!re.is_match("light.kitchen"));
        assert!(re.is_match("Kitchen_sensor"));
    }

    #[test]
    fn safe_regex_empty_pattern_errors() {
        assert!(safe_regex("", false).is_err());
    }

    #[test]
    fn safe_regex_too_long_errors() {
        let long = "a".repeat(201);
        assert!(safe_regex(&long, false).is_err());
    }

    #[test]
    fn safe_regex_invalid_syntax_errors() {
        assert!(safe_regex("[invalid", false).is_err());
    }

    // No ReDoS test needed — Rust's regex crate is O(n) by design

    // --- validate_service_format ---

    #[test]
    fn valid_service_format() {
        let (domain, service) = validate_service_format("light.turn_on").unwrap();
        assert_eq!(domain, "light");
        assert_eq!(service, "turn_on");
    }

    #[test]
    fn service_format_with_dots_in_service() {
        let (domain, service) = validate_service_format("some.complex.name").unwrap();
        assert_eq!(domain, "some");
        assert_eq!(service, "complex.name");
    }

    #[test]
    fn invalid_service_format_no_dot() {
        assert!(validate_service_format("nodot").is_err());
    }

    #[test]
    fn invalid_service_format_empty() {
        assert!(validate_service_format("").is_err());
    }

    #[test]
    fn invalid_service_format_trailing_dot() {
        assert!(validate_service_format("light.").is_err());
    }

    #[test]
    fn invalid_service_format_leading_dot() {
        assert!(validate_service_format(".turn_on").is_err());
    }

    // --- validate_reload_component ---

    #[test]
    fn valid_reload_components() {
        assert_eq!(
            validate_reload_component(Some("automations")).unwrap(),
            "automations"
        );
        assert_eq!(
            validate_reload_component(Some("scripts")).unwrap(),
            "scripts"
        );
        assert_eq!(validate_reload_component(Some("scenes")).unwrap(), "scenes");
        assert_eq!(validate_reload_component(Some("all")).unwrap(), "all");
    }

    #[test]
    fn reload_component_default_is_all() {
        assert_eq!(validate_reload_component(None).unwrap(), "all");
    }

    #[test]
    fn invalid_reload_component() {
        assert!(validate_reload_component(Some("invalid")).is_err());
    }

    // --- parse_json_option ---

    #[test]
    fn parse_valid_json() {
        let val = parse_json_option(r#"{"entity_id":"light.kitchen"}"#, "data").unwrap();
        assert_eq!(val["entity_id"], "light.kitchen");
    }

    #[test]
    fn parse_json_empty_errors() {
        assert!(parse_json_option("", "data").is_err());
    }

    #[test]
    fn parse_json_single_quotes_hint() {
        let err = parse_json_option("{'key': 'value'}", "data").unwrap_err();
        assert!(err.to_string().contains("double quotes"));
    }

    #[test]
    fn parse_json_object_valid() {
        let val = parse_json_object_option(r#"{"key":"value"}"#, "data").unwrap();
        assert!(val.is_object());
    }

    #[test]
    fn parse_json_object_rejects_array() {
        let err = parse_json_object_option(r#"[1,2,3]"#, "data").unwrap_err();
        assert!(
            err.to_string().contains("object"),
            "should mention object: {err}"
        );
    }

    #[test]
    fn parse_json_object_rejects_string() {
        let err = parse_json_object_option(r#""hello""#, "target").unwrap_err();
        assert!(
            err.to_string().contains("object"),
            "should mention object: {err}"
        );
    }

    #[test]
    fn parse_json_invalid_syntax() {
        let err = parse_json_option("{bad json}", "data").unwrap_err();
        assert!(err.to_string().contains("data"));
    }
}
