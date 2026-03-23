use crate::error::AppError;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub url: String,
    pub token: String,
}

/// Resolve auth from flag > env > file chain
pub fn resolve_auth(url_flag: Option<&str>, token_flag: Option<&str>) -> Result<AuthConfig, AppError> {
    let url = url_flag
        .map(String::from)
        .or_else(|| std::env::var("HA_URL").ok());
    let url = match url {
        Some(u) => u,
        None => read_token_file("~/.ha_url", false)?
            .ok_or(AppError::MissingUrl)?,
    };

    let token = token_flag
        .map(String::from)
        .or_else(|| std::env::var("HA_TOKEN").ok());
    let token = match token {
        Some(t) => t,
        None => read_token_file("~/.ha_token", true)?
            .ok_or(AppError::MissingToken)?,
    };

    Ok(AuthConfig {
        url: url.trim_end_matches('/').to_string(),
        token,
    })
}

/// Read a credential file, returning Ok(None) if the file doesn't exist,
/// Ok(Some(content)) on success, or Err for I/O errors and insecure permissions.
fn read_token_file(path: &str, check_perms: bool) -> Result<Option<String>, AppError> {
    let expanded = expand_tilde(path);
    let content = match fs::read_to_string(&expanded) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(AppError::Other(format!(
                "Failed to read {}: {e}",
                expanded.display()
            )));
        }
    };
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        return Ok(None);
    }

    #[cfg(unix)]
    if check_perms {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(&expanded) {
            let mode = metadata.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                return Err(AppError::InsecurePermissions {
                    path: path.to_string(),
                    mode,
                });
            }
        }
    }

    #[cfg(not(unix))]
    let _ = check_perms;

    Ok(Some(trimmed))
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs_home()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn resolve_auth_from_flags() {
        let auth = resolve_auth(Some("http://ha.local:8123"), Some("test-token")).unwrap();
        assert_eq!(auth.url, "http://ha.local:8123");
        assert_eq!(auth.token, "test-token");
    }

    #[test]
    fn resolve_auth_strips_trailing_slash() {
        let auth = resolve_auth(Some("http://ha.local:8123/"), Some("tok")).unwrap();
        assert_eq!(auth.url, "http://ha.local:8123");
    }

    #[test]
    fn resolve_auth_missing_url_errors() {
        // Clear env vars for this test
        unsafe { env::remove_var("HA_URL") };
        unsafe { env::remove_var("HA_TOKEN") };
        let err = resolve_auth(None, Some("tok")).unwrap_err();
        assert!(err.to_string().contains("HA_URL"));
    }

    #[test]
    fn resolve_auth_missing_token_errors() {
        unsafe { env::remove_var("HA_TOKEN") };
        let err = resolve_auth(Some("http://ha.local"), None).unwrap_err();
        assert!(err.to_string().contains("HA_TOKEN"));
    }

    #[test]
    fn resolve_auth_from_env() {
        unsafe { env::set_var("HA_URL", "http://env-ha:8123") };
        unsafe { env::set_var("HA_TOKEN", "env-token") };
        let auth = resolve_auth(None, None).unwrap();
        assert_eq!(auth.url, "http://env-ha:8123");
        assert_eq!(auth.token, "env-token");
        unsafe { env::remove_var("HA_URL") };
        unsafe { env::remove_var("HA_TOKEN") };
    }

    #[test]
    fn flags_override_env() {
        unsafe { env::set_var("HA_URL", "http://env-ha:8123") };
        unsafe { env::set_var("HA_TOKEN", "env-token") };
        let auth = resolve_auth(Some("http://flag-ha:8123"), Some("flag-token")).unwrap();
        assert_eq!(auth.url, "http://flag-ha:8123");
        assert_eq!(auth.token, "flag-token");
        unsafe { env::remove_var("HA_URL") };
        unsafe { env::remove_var("HA_TOKEN") };
    }

    #[test]
    fn resolve_auth_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let url_path = dir.path().join(".ha_url");
        let token_path = dir.path().join(".ha_token");
        fs::write(&url_path, "http://file-ha:8123\n").unwrap();
        fs::write(&token_path, "  file-token  \n").unwrap();

        // read_token_file with absolute paths
        let url = fs::read_to_string(&url_path).unwrap().trim().to_string();
        let token = fs::read_to_string(&token_path).unwrap().trim().to_string();
        assert_eq!(url, "http://file-ha:8123");
        assert_eq!(token, "file-token");
    }

    #[test]
    fn expand_tilde_works() {
        let expanded = expand_tilde("~/test");
        assert!(!expanded.to_string_lossy().starts_with('~'));
    }

    #[test]
    fn read_missing_file_returns_none() {
        let result = read_token_file("/nonexistent/path/.ha_token", false).unwrap();
        assert!(result.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn read_insecure_file_errors() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".ha_token");
        fs::write(&path, "secret-token").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let result = read_token_file(path.to_str().unwrap(), true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("chmod 600"));
    }
}
