use std::io::IsTerminal;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Terminal styling — returns empty strings when stdout is not a TTY.
pub struct Style {
    pub dim: &'static str,
    pub bold: &'static str,
    pub green: &'static str,
    pub red: &'static str,
    pub yellow: &'static str,
    pub cyan: &'static str,
    pub reset: &'static str,
}

impl Style {
    pub fn detect() -> Self {
        if Self::should_color() {
            Self {
                dim: "\x1b[2m",
                bold: "\x1b[1m",
                green: "\x1b[32m",
                red: "\x1b[31m",
                yellow: "\x1b[33m",
                cyan: "\x1b[36m",
                reset: "\x1b[0m",
            }
        } else {
            Self {
                dim: "",
                bold: "",
                green: "",
                red: "",
                yellow: "",
                cyan: "",
                reset: "",
            }
        }
    }

    pub fn header(&self, text: &str) -> String {
        format!("{}◆ {}{}", self.bold, text, self.reset)
    }

    pub fn pass(&self, text: &str) -> String {
        format!("{}✓{} {}", self.green, self.reset, text)
    }

    pub fn fail(&self, text: &str) -> String {
        format!("{}✗{} {}", self.red, self.reset, text)
    }

    pub fn warn(&self, text: &str) -> String {
        format!("{}▲{} {}", self.yellow, self.reset, text)
    }

    pub fn separator(&self, width: usize) -> String {
        format!("{}{}{}", self.dim, "─".repeat(width), self.reset)
    }

    /// Respect NO_COLOR (https://no-color.org), TERM=dumb, and TTY detection.
    fn should_color() -> bool {
        if !std::io::stdout().is_terminal() {
            return false;
        }
        if std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()) {
            return false;
        }
        if std::env::var("TERM").as_deref() == Ok("dumb") {
            return false;
        }
        true
    }

    /// Key-value line with dimmed label, left-padded to `width`.
    pub fn kv(&self, label: &str, width: usize, value: &str) -> String {
        format!(
            "  {}{:<width$}{}  {}",
            self.dim,
            label,
            self.reset,
            value,
            width = width,
        )
    }
}

/// Run a future with a braille spinner on stderr.
/// Only shows when `show` is true and stderr is a TTY.
pub async fn with_spinner<F, T>(message: &str, show: bool, future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    if !show || !std::io::stderr().is_terminal() {
        return future.await;
    }

    eprint!("\x1b[?25l"); // hide cursor

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let msg = message.to_string();

    let task = tokio::spawn(async move {
        let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let mut i = 0usize;
        while r.load(Ordering::Relaxed) {
            eprint!("\r\x1b[2m{} {}\x1b[0m\x1b[K", frames[i % frames.len()], msg);
            i = i.wrapping_add(1);
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
    });

    let result = future.await;

    running.store(false, Ordering::Relaxed);
    let _ = task.await;
    eprint!("\r\x1b[K\x1b[?25h"); // clear line, show cursor

    result
}

/// Format number with comma separators: 10258 → "10,258"
pub fn fmt_num(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format number for compact/LLM mode: 999 → "999", 10258 → "10.3k", 1200000 → "1.2M"
pub fn fmt_num_compact(n: usize) -> String {
    if n >= 999_950 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Truncate to max display chars, appending '…' if needed.
pub fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let truncated: String = chars[..max - 1].iter().collect();
        format!("{truncated}…")
    }
}

/// Extract filename from a path string.
pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Color a state value based on its meaning.
pub fn state_color<'a>(state: &str, s: &'a Style) -> (&'a str, &'a str) {
    match state {
        "unavailable" => (s.red, s.reset),
        "unknown" => (s.yellow, s.reset),
        "on" => (s.green, s.reset),
        "off" => (s.dim, s.reset),
        _ => ("", ""),
    }
}

/// Render entities as an aligned table.
pub fn entity_table(entities: &[serde_json::Value], title: &str, s: &Style) -> String {
    let mut out = format!("{}\n\n", s.header(title));

    if entities.is_empty() {
        out.push_str(&format!("  {}No entities found{}\n", s.dim, s.reset));
        return out;
    }

    // Collect rows
    let rows: Vec<(&str, &str, &str)> = entities
        .iter()
        .map(|e| {
            let id = e.get("entity_id").and_then(|v| v.as_str()).unwrap_or("");
            let state = e.get("state").and_then(|v| v.as_str()).unwrap_or("");
            let name = e
                .pointer("/attributes/friendly_name")
                .or_else(|| e.get("friendly_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            (id, state, name)
        })
        .collect();

    // Column widths (capped)
    let id_w = rows.iter().map(|r| r.0.len()).max().unwrap_or(10).min(50);
    let st_w = rows
        .iter()
        .map(|r| r.1.len())
        .max()
        .unwrap_or(5)
        .clamp(5, 14);

    // Header row
    out.push_str(&format!(
        "  {}{:<id_w$}  {:<st_w$}  {}{}\n",
        s.dim, "Entity ID", "State", "Name", s.reset,
    ));
    out.push_str(&format!("  {}\n", s.separator(id_w + st_w + 30)));

    for (id, state, name) in &rows {
        let display_id = truncate(id, id_w);
        let display_name = truncate(name, 40);
        let (cs, ce) = state_color(state, s);
        // Pad the state *before* wrapping in color so visible width is correct
        let state_padded = format!("{:<st_w$}", state);
        out.push_str(&format!(
            "  {:<id_w$}  {}{}{}  {}\n",
            display_id, cs, state_padded, ce, display_name,
        ));
    }

    out.push_str(&format!(
        "\n  {}{} entities{}\n",
        s.dim,
        fmt_num(rows.len()),
        s.reset,
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_formatting() {
        assert_eq!(fmt_num(0), "0");
        assert_eq!(fmt_num(999), "999");
        assert_eq!(fmt_num(1000), "1,000");
        assert_eq!(fmt_num(10258), "10,258");
        assert_eq!(fmt_num(1000000), "1,000,000");
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world!", 5), "hell…");
        assert_eq!(truncate("anything", 0), "");
    }

    #[test]
    fn basename_extraction() {
        assert_eq!(basename("/Users/foo/bar.yaml"), "bar.yaml");
        assert_eq!(basename("bar.yaml"), "bar.yaml");
    }

    #[test]
    fn fmt_num_compact_small() {
        assert_eq!(fmt_num_compact(0), "0");
        assert_eq!(fmt_num_compact(999), "999");
    }

    #[test]
    fn fmt_num_compact_thousands() {
        assert_eq!(fmt_num_compact(1000), "1.0k");
        assert_eq!(fmt_num_compact(1500), "1.5k");
        assert_eq!(fmt_num_compact(10258), "10.3k");
    }

    #[test]
    fn fmt_num_compact_boundary_uses_m_not_1000k() {
        assert_eq!(fmt_num_compact(999_949), "999.9k");
        assert_eq!(fmt_num_compact(999_950), "1.0M");
        assert_eq!(fmt_num_compact(999_999), "1.0M");
    }

    #[test]
    fn fmt_num_compact_millions() {
        assert_eq!(fmt_num_compact(1000000), "1.0M");
        assert_eq!(fmt_num_compact(1500000), "1.5M");
        assert_eq!(fmt_num_compact(12345678), "12.3M");
    }

    #[test]
    fn style_produces_output() {
        let s = Style::detect();
        assert!(s.header("test").contains("test"));
        assert!(s.pass("ok").contains("ok"));
        assert!(s.fail("bad").contains("bad"));
        assert!(s.warn("hmm").contains("hmm"));
    }
}
