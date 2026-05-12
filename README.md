# homeassist

Token-efficient Home Assistant CLI for AI agents and humans.

A single compiled binary that replaces hundreds of lines of shell scripts with
sub-second performance. Designed for both LLM agent workflows (compact JSON output)
and interactive terminal use (styled human output with spinners, colors, and aligned tables).

## Install

### Standalone CLI

```bash
cargo install --git https://github.com/jsnyder/homeassist
```

Or from a local checkout:

```bash
cargo install --path .
```

Requires Rust 1.85+.

### Claude Code Plugin

Install as a Claude Code plugin to get skills (contextual guidance for AI agents)
alongside the CLI:

```bash
/plugin marketplace add jsnyder/homeassist
/plugin install homeassist@homeassist-marketplace
```

The plugin provides two skills:
- **homeassist** — entity queries, services, logs, system health
- **ha-deploy** — pre-deploy validation and post-deploy verification workflow

## Authentication

Set your Home Assistant URL and long-lived access token:

```bash
export HA_URL=http://homeassistant.local:8123
export HA_TOKEN=your_token_here
```

Or use flags (`--url`, `--token`) or files (`~/.ha_url`, `~/.ha_token`).

## Quick Start

```bash
homeassist health                                    # Connection check
homeassist entities list --domain light              # List lights
homeassist entities list --state unavailable          # Find broken entities
homeassist services call light.turn_on --data '{"entity_id":"light.kitchen"}'
homeassist logs errors --tail 10                     # Recent errors (via WebSocket)
homeassist inspect                                   # System health audit
```

## Commands

| Command | Description |
|---------|-------------|
| `entities list` | List entities with `--domain`, `--state`, `--pattern`, `--name` filters |
| `entities get <id>` | Get single entity state and attributes |
| `entities search <pattern>` | Search entities by regex |
| `services list [domain]` | List available services |
| `services call <domain.service>` | Call a service with `--data` and `--target` |
| `templates render "<template>"` | Render a Jinja2 template |
| `config check` | Validate HA configuration |
| `config reload [component]` | Reload automations, scripts, scenes, or all |
| `history get <id>` | Entity state history (`--hours`) |
| `logs errors` | System log entries (`--tail`, `--pattern`) |
| `logbook get <id>` | Entity event timeline |
| `events fire <type>` | Fire a custom event |
| `automations list` | List all automations |
| `automations trigger <id>` | Trigger an automation |
| `scripts list` | List all scripts |
| `scripts run <id>` | Run a script |
| `batch` | Execute multiple commands from JSONL |
| `diff` | Entities that changed state recently |
| `watch <id>` | Watch entity for state changes |
| `inspect` | Audit system health |
| `validate [path]` | Validate HA YAML config files (`--check-entities`, `--check-registry`) |
| `verify` | Post-deploy health verification |
| `stats` | System status overview (`--dashboard`, `--init`) |
| `health` | Server connection status |
| `completions <shell>` | Generate shell completions |

## Output Modes

homeassist auto-detects the best output mode:

- **Interactive terminal** → Human-readable with colors, alignment, spinners
- **Claude Code** (`CLAUDECODE=1`) → Compact tab-separated for token savings
- **Piped/scripted** → JSON

Override with `--human`, `--compact`, or `--no-compact`. Use `--limit N` to cap
entity lists in any output mode.

## Deployment Validation

homeassist replaces ~1,200 lines of shell-based deployment validation with a
compiled binary that runs **40-170x faster**:

| Step | bash script | homeassist | Speedup |
|------|------------:|-----------:|--------:|
| Local YAML validation (157 files) | ~30s | **0.12s** | ~250x |
| Validation + entity ref checks | ~35s | **0.25s** | ~140x |
| Full validate + verify pipeline | ~42s | **1.3s** | ~32x |

```bash
homeassist validate ./packages --check-entities --check-registry || exit 1   # pre-deploy
homeassist verify --baseline snapshot.json                                    # post-deploy
```

See [docs/deployment-validation.md](docs/deployment-validation.md) for the full
comparison with `deploy-unified.sh`, migration path, and check-by-check mapping.

## WebSocket Support

homeassist uses the Home Assistant WebSocket API for commands not available via
REST. This includes:

- **System logs** (`system_log/list`) — works on all HA installs including HAOS
  and container setups where the REST `/api/error_log` endpoint returns 404
- **Entity registry** (`config/entity_registry/list`) — orphaned entry detection
  via `validate --check-registry`

The WebSocket client handles the full HA auth handshake with 30-second timeouts
and automatic resource cleanup.

## Architecture

- **Language**: Rust (2024 edition)
- **Async runtime**: Tokio
- **HTTP client**: reqwest with rustls-tls
- **WebSocket**: tokio-tungstenite
- **CLI framework**: clap (derive)
- **Tests**: 209 unit/integration tests using real WebSocket servers (no mocks)

## License

MIT
