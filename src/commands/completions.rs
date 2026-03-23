use crate::error::AppError;
use clap::CommandFactory;
use clap_complete::{Shell, generate};

/// Generate shell completions for the given shell.
pub fn generate_completions(shell: &str) -> Result<String, AppError> {
    let shell = match shell.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        _ => {
            return Err(AppError::Other(format!(
                "Unknown shell: {shell}. Supported: bash, zsh, fish, powershell, elvish"
            )));
        }
    };

    let mut cmd = crate::Cli::command();
    let mut buf = Vec::new();
    generate(shell, &mut cmd, "homeassist", &mut buf);
    String::from_utf8(buf)
        .map_err(|e| AppError::Other(format!("Failed to generate completions: {e}")))
}
