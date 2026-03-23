# CLAUDE.md

## Build & Test

```bash
cargo build            # Build
cargo test             # Run all tests (45 unit tests)
cargo clippy           # Lint
cargo fmt -- --check   # Format check
```

## Constraints

- Output JSON by default, `--compact` for LLM token savings, `--human` for readable
- Auth: `--url`/`--token` flags > `HA_URL`/`HA_TOKEN` env vars > `~/.ha_url`/`~/.ha_token` files
- `env::set_var`/`remove_var` require `unsafe` blocks (Rust 1.63+)
- Regex patterns capped at 200 chars; `regex` crate guarantees O(n) (no ReDoS)
- Tests run with `--test-threads=1` if env var tests interfere (they currently don't)
