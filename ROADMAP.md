# homeassist Roadmap

## Completed

### Phase 0: Core CLI
- Entity operations (list, get, search with regex/domain/name filters)
- Service operations (list, call with JSON data/target)
- Template rendering
- Config check and reload
- Health check
- JSON/compact/human output modes
- Auth chain: flags > env vars > files

### Phase 1: Security Hardening
- Percent-encoding for path segments
- File permission checks on token files
- Regex length cap (200 chars, O(n) guaranteed)
- Sanitized error messages (no token/URL leaks)
- Lazy auth resolution

### Phase 2: Extended API
- History (with compact numeric summaries)
- Error logs (with tail/pattern filtering)
- Logbook (event timeline)
- Events (fire custom events)
- Automations (list, trigger)
- Scripts (list, run)

### Phase 3: Claude Code Plugin
- Plugin packaging (.claude-plugin/)
- Skills (homeassist, ha-automation-tdd)
- Slash commands (health, entities, test, deploy)
- Pre-deploy hook

### Phase 4: Advanced Features
- Batch (JSONL multi-command execution)
- Diff (entity state changes by time window)
- Shell completions (bash, zsh, fish, powershell, elvish)
- Watch (poll entity for state changes with timeout/interval/target)
- Inspect (system health audit: unavailable/unknown entities, domain summary)

## Future: High-Value Features

Prioritized by frequency of manual workarounds observed in past sessions.

### 1. Automation Traces (`homeassist traces`)
**Value**: High — Claude currently writes raw Python websocket scripts to poll `trace/list` and `trace/get` for debugging automations. This was the most complex workaround pattern found.

**What it would do**:
- `homeassist traces list <automation_id>` — List recent traces with timestamps, state, execution result
- `homeassist traces get <automation_id> --run-id <id>` — Get full trace detail (conditions evaluated, actions executed, errors)
- `homeassist traces latest <automation_id>` — Shortcut for most recent trace

**Challenge**: Requires websocket API (`trace/list`, `trace/get` are websocket-only). Would need `tokio-tungstenite` or similar dependency. Significant implementation effort.

### 2. Repairs / Issues (`homeassist repairs`)
**Value**: High — "check repairs" is a recurring request. Currently requires UI or websocket.

**What it would do**:
- `homeassist repairs list` — Show all active repair issues
- `homeassist repairs dismiss <id>` — Dismiss a repair
- Compact mode: one line per repair with severity

**Challenge**: Repairs API is websocket-only (`repairs/list_issues`). Same websocket dependency as traces.

### 3. Areas & Devices (`homeassist areas` / `homeassist devices`)
**Value**: Medium — Entity-to-area mapping, device registry info. Useful for spatial queries ("what devices are in the kitchen?") and for understanding entity groupings.

**What it would do**:
- `homeassist areas list` — List all areas
- `homeassist areas entities <area_id>` — List entities in an area
- `homeassist devices list [--domain X]` — List devices with manufacturer/model
- `homeassist devices get <device_id>` — Device detail with all entities

**Challenge**: Area/device registry is websocket-only. REST API only has entity states.

### 4. Labels & Categories (`homeassist labels`)
**Value**: Medium — HA 2024+ added labels for organizing entities. Useful for bulk operations.

**What it would do**:
- `homeassist labels list` — List all labels
- `homeassist labels entities <label>` — Entities with a given label

**Challenge**: Websocket-only API.

### 5. Config Profiles
**Value**: Low-Medium — Store named HA connection configs for switching between instances.

**What it would do**:
- `homeassist profile set prod --url http://ha:8123 --token xxx`
- `homeassist profile use prod`
- `homeassist --profile staging health`

**Challenge**: Minimal — file-based config, no new API deps. Low priority since most users have one instance.

### 6. Addon Management (`homeassist addons`)
**Value**: Low — Supervisor API for listing/starting/stopping addons.

**What it would do**:
- `homeassist addons list`
- `homeassist addons restart <slug>`
- `homeassist addons logs <slug>`

**Challenge**: Supervisor API is separate from core REST API, requires different auth.

## Phase 5: Safe Deployment System

A general-purpose deployment pipeline for any Home Assistant installation, built into the
CLI. Designed to replace ad-hoc rsync/scp scripts with a single, opinionated workflow
that has failsafes at every step.

Inspired by the battle-tested `deploy-unified.sh` (~1,800 lines) from the home_assistant
project, but generalized for any user managing HA config in git.

### Design Principles

- **Stop-the-line**: Any validation failure halts deployment. No partial deploys.
- **Reversible by default**: Backup before every deploy, automatic rollback on failure.
- **Verify what you deploy**: Post-deploy checks confirm the system is healthy.
- **Transport-agnostic**: SSH, Docker, or local — same workflow, different transport.
- **Progressive disclosure**: Simple `homeassist deploy` works with defaults; power users
  can configure everything.

### Tier 1: Validate & Verify (no SSH needed)

Local YAML validation + API-based verification. High value, zero transport complexity.

**`homeassist validate [path]`** — Pre-deploy validation suite:
- YAML syntax check (valid YAML, proper indentation)
- Automation structure (triggers/conditions/actions present and well-formed)
- Template format (balanced Jinja delimiters, no common errors)
- Duplicate entity ID detection across files
- Sensor platform validation (required fields present)
- Entity reference check against live HA (do referenced entities exist?)
- Common error patterns (hardcoded IPs, missing required fields, deprecated syntax)
- Circular reference detection in template sensors

**`homeassist verify [--baseline <file>]`** — Post-deploy health check:
- Connection health
- Unavailable entity count delta (before vs after deploy)
- Critical entity existence check (from config)
- Automation state check (none went "unavailable")
- Snapshot baseline for future comparisons

**Dependencies**: `serde_yaml` for YAML parsing. ~550 lines.

### Tier 2: Deploy Orchestration

The full pipeline: validate → backup → sync → check → reload → verify → rollback.

**`homeassist deploy [--dry-run] [--method ssh|docker|local]`**

Pipeline steps:
1. `homeassist validate .` — local validation (abort on failure)
2. Create timestamped backup on target
3. Sync config files to target (rsync for SSH, cp for local, docker cp for Docker)
4. `homeassist config check` — HA validates its own config (abort + rollback on failure)
5. `homeassist config reload all` or restart (based on what changed)
6. Wait for HA startup (poll health endpoint)
7. `homeassist verify` — post-deploy checks (rollback on failure)
8. Update issue baseline on success

**Transport methods** (each ~50-100 lines):
- `ssh` — rsync over SSH (most common for HA OS)
- `docker` — docker cp + docker exec
- `local` — direct filesystem copy (for supervised/core installs)

**`homeassist deploy rollback`** — Restore from most recent backup.

### Configuration: `.homeassist.toml`

Per-project config file in the user's HA config repo:

```toml
[deploy]
method = "ssh"                    # ssh | docker | local
host = "homeassistant.local"
user = "root"
ssh_key = "~/.ssh/id_rsa"        # optional, uses ssh-agent by default
config_path = "/config"           # remote config directory
packages_dir = "packages"         # local packages directory
exclude = ["*.disabled", "archive/*", ".git/*"]

[verify]
critical_entities = [             # entities that MUST exist after deploy
    "climate.thermostat",
    "binary_sensor.front_door",
]
max_unavailable_delta = 5         # fail if unavailable count increases by more
startup_timeout = 120             # seconds to wait for HA to come back up

[validate]
check_entity_refs = true          # verify entity references against live HA
check_circular_refs = true        # detect template circular references
```

### Implementation Estimate

| Component | Lines | New dependencies |
|-----------|-------|-----------------|
| YAML validation suite | ~400 | `serde_yaml` |
| Verify command | ~150 | none |
| Deploy orchestration | ~200 | none |
| SSH transport | ~100 | shells out to ssh/rsync |
| Docker transport | ~60 | shells out to docker |
| Local transport | ~40 | `std::fs` |
| Config file parsing | ~80 | `toml` crate |
| **Total** | **~1,030** | 2 crates |

### Rollout Plan

1. **Phase 5a**: `validate` + `verify` commands (no transport, no config file)
2. **Phase 5b**: `.homeassist.toml` config + `deploy` with SSH transport
3. **Phase 5c**: Docker and local transports
4. **Phase 5d**: Issue tracking (baseline comparisons over time)

### Phase 5a: Deployment Validation (Tier 1)
- `validate` command — YAML syntax, automation structure, Jinja balance, duplicates,
  sensor platforms, common errors, cruft detection, entity reference checks
- `verify` command — config validity, entity/automation audit, baseline delta
- 40-170x faster than equivalent bash script validation
- See [docs/deployment-validation.md](docs/deployment-validation.md)

### WebSocket Support
- `tokio-tungstenite` client with HA auth handshake, 30s timeouts, resource cleanup
- `system_log/list` for universal log access (fixes HAOS REST 404)
- 10 tests using real local WebSocket servers (no mocks)
- Unlocks traces, repairs, areas, devices, labels (all WS-only APIs)
