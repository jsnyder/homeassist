# Output System Design

Reference for the homeassist CLI output system. Covers philosophy, mode
detection, compact format specifications, color rules, and anti-patterns.

Implemented primarily in `src/output.rs` (mode detection, formatting) and
`src/ui.rs` (color, typography, style).

---

## 1. Philosophy

The CLI serves three audiences with one binary:

| Audience | Needs | Mode |
|----------|-------|------|
| Humans at a terminal | Color, structure, readable numbers | Human (TTY) |
| LLMs / AI agents | Minimal tokens, parseable, no chrome | Compact |
| Scripts / pipelines | Stable schema, machine-readable | JSON (piped) |

Core principles:

- **Token-efficiency** -- compact mode exists specifically to minimize LLM
  token consumption while preserving all actionable information.
- **Color = meaning** -- every color carries semantic weight (green = ok,
  red = error). Color is never decorative.
- **stdout / stderr separation** -- data goes to stdout, diagnostics and
  errors go to stderr. Scripts can rely on this.
- **Respect the environment** -- honor `NO_COLOR`, `TERM=dumb`, and
  non-TTY detection. Never force escape codes where they are unwanted.

---

## 2. Output Modes

### Detection logic

Evaluated top-to-bottom, first match wins:

```
--human flag          --> Human mode
--no-compact flag     --> (fall through to TTY check)
--compact flag        --> Compact mode
CLAUDECODE=1 env var  --> Compact mode
stdout is a TTY       --> Human mode
stdout is a pipe      --> JSON mode
```

### Mode characteristics

| Property | Human | Compact | JSON |
|----------|-------|---------|------|
| Color | Yes (TTY) | No | No |
| Structure | Headers, separators, icons | Tab-delimited, one record/line | Structured objects |
| Numbers | Comma-separated | Abbreviated (k/M) | Raw integers |
| Target | Terminal users | LLMs, `--compact` | Piped consumers |

---

## 3. Compact Mode Design

Compact mode minimizes tokens while keeping output grep-friendly and
unambiguous.

Rules:

- **One record per line.** Each logical item occupies exactly one line.
- **Tab-delimited fields.** Fields within a line are separated by `\t`.
- **No chrome.** No headers, boxes, separators, or decorative characters.
- **Truncate at 120 characters.** Long values are cut with `...` to cap
  line width.
- **Units without space.** Write `42ms`, `3.2k`, `98%` -- not `42 ms`.
- **Default 50-item cap.** Lists longer than 50 items are truncated with a
  final `[+N more]` line.

---

## 4. Per-Command Compact Format Specs

| Command | Budget | Format |
|---------|--------|--------|
| `health` | <30 tokens | `v2024.12.1\tconnected\tHome` |
| `stats` | <100 sys + ~15/section | `sys\tv...\tconnected\tcomp:N` / `ent\ttotal:N\tunavail:N...` |
| `entities list` | ~5/entity | `entity_id\tstate` |
| `entities get` | <50 | Single-line JSON |
| `automations list` | ~5/entity | `entity_id\tstate` |
| `inspect` | <100 + ~5/problem | `total:N\tunavail:N\tunk:N` |
| `history get` | <80 | `min:N\tmax:N\tavg:N\tlast:N\tunit:X\tsamples:N` |
| `diff` | ~8/changed | `entity_id\told->new\ttime` |
| `logbook get` | ~10/event | `time\tentity_id\tstate\tmessage` |
| `logs errors` | ~15/entry | `LEVEL (component) [Nx]: message` |
| `validate` | <20/finding | `severity\tfile:line\tmessage` |
| `verify` | <50 | Pass/fail summary |
| `services list` | ~3/service | `domain.service` per line |
| `services call` | <10 | `ok\tentity_id` |
| `config check` | <20 | `ok` or `error: message` |
| `templates render` | varies | Raw string |

Token counts are approximate and measured with the `cl100k_base` tokenizer.

---

## 5. Token Budget Guidelines

| Scenario | Target |
|----------|--------|
| Single entity query | <50 tokens |
| Entity list (default cap) | <250 tokens |
| System status (`health`, `stats`) | <100 tokens |
| Validation finding | <20 tokens/finding |
| Error log entries | <50 tokens |

These budgets guide compact-mode formatting decisions. When a format
change would push output over budget, prefer truncation or abbreviation.

---

## 6. Abbreviations (Compact Mode Only)

Compact mode uses shortened forms to save tokens. These abbreviations are
never used in human or JSON modes.

| Full form | Abbreviation |
|-----------|-------------|
| unavailable | unavail |
| unknown | unk |
| automation | auto |
| binary_sensor | bin |
| temperature | temp |
| components | comp |

---

## 7. Numeric Formatting

| Range | Compact | Human |
|-------|---------|-------|
| <1,000 | Raw (`42`) | Raw (`42`) |
| 1,000 -- 999,999 | `N.Nk` (`12.3k`) | Comma (`12,345`) |
| >=1,000,000 | `N.NM` (`1.2M`) | Comma (`1,234,567`) |

Special cases:

- **Latency**: integer + `ms` (`42ms`), no decimal.
- **Percentages**: integer + `%` (`98%`), no decimal.

---

## 8. Color Rules

All colors use the **ANSI 16-color palette** only. No 256-color or
truecolor sequences. Implemented via the `Style` struct in `src/ui.rs`.

| Color | Meaning |
|-------|---------|
| Green | OK, on, connected, passing |
| Red | Error, unavailable, failing |
| Yellow | Warning, unknown state |
| Cyan | Metadata (reserved for labels/keys) |
| Dim | Secondary info, off state |
| Bold | Headers, emphasis |

### When color is disabled

Color output is suppressed when any of these conditions is true:

- stdout is not a TTY
- `NO_COLOR` environment variable is set (any value)
- `TERM=dumb`

---

## 9. Typography

Human mode uses a small set of Unicode characters for visual structure.
These are never emitted in compact or JSON modes.

| Element | Character | Usage |
|---------|-----------|-------|
| Section header | `◆` (diamond) | Groups of related output |
| Success | `✓` (check) | Passed checks, connected |
| Failure | `✗` (cross) | Failed checks, errors |
| Warning | `▲` (triangle) | Warnings, degraded state |
| Key-value labels | Dim text | `state: on`, `version: 2024.12` |
| Separators | `─` (horizontal rule, dim) | Between sections |

---

## 10. Error Conventions

All error output follows these rules:

- **stderr only.** Errors never appear on stdout.
- **Human-readable.** Plain English, no internal codes or struct dumps.
- **Suggest fixes.** When possible, include what the user should do next
  (e.g., "run `homeassist health` to check connectivity").
- **No stack traces.** Backtraces are for `RUST_BACKTRACE=1`, never the
  default.
- **Non-zero exit.** Every error path sets a non-zero exit code.

---

## 11. Anti-Patterns

Things this CLI intentionally avoids:

| Anti-pattern | Why |
|--------------|-----|
| Rainbow / gradient text | Color must carry meaning, not decoration |
| Box-drawing characters | Wastes tokens, breaks in narrow terminals |
| Emoji in data fields | Ambiguous width, breaks column alignment |
| Progress bars / spinners | CLI calls are short; adds complexity for no benefit |
| Verbose output by default | Wastes tokens; use `--human` for detail |
| Pretty-printed JSON in compact | Defeats the purpose of compact mode |
