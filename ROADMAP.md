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

## Architecture Decision: Websocket Support

Features 1-4 all require websocket access. Adding websocket support would unlock a significant chunk of HA's API that is currently inaccessible via REST. The recommended approach:

1. Add `tokio-tungstenite` dependency
2. Create a `ws_client.rs` module with auth handshake and request/response pattern
3. Implement traces first (highest value, most complex workaround it replaces)
4. Repairs, areas, devices, labels follow naturally once websocket infra exists

This is the single highest-leverage architectural change for future development.
