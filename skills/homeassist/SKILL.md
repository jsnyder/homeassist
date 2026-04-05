---
name: homeassist
description: Use when querying or controlling Home Assistant — entities, services, templates, logs, automations, system status. Also for pre-deploy YAML validation, post-deploy verification, and system status dashboards.
---

# homeassist CLI

Compiled Rust CLI for Home Assistant. Auto-detects Claude Code for compact output; interactive terminals get styled human output.

## Entity Operations

```bash
homeassist entities list --domain light              # By domain
homeassist entities list --state unavailable          # Exact state match
homeassist entities list --pattern "hvac.*power"     # Regex on id/name
homeassist entities list --name "kitchen"            # Friendly name regex
homeassist entities list --domain sensor --state on  # Combine filters
homeassist entities get sensor.kitchen_temperature   # Single entity detail
homeassist entities search "fountain"                # Search id + friendly_name
```

## Services

```bash
homeassist services list climate                     # Domain signatures
homeassist services call light.turn_on --data '{"entity_id":"light.kitchen"}'
homeassist services call light.turn_on --target '{"area_id":"kitchen"}'
```

## System Status & Monitoring

```bash
homeassist stats                           # System overview: version, entities, automations, errors
homeassist stats --dashboard my.yaml       # Custom dashboard with sections + templates
homeassist stats --init > dashboard.yaml   # Auto-generate dashboard config from HA state
homeassist health                          # Connection + version
homeassist inspect                         # Unavailable/unknown audit, domain summary
homeassist logs errors --tail 10           # System log via WebSocket (all installs)
homeassist logs errors --pattern "zigbee"  # Filter by regex
homeassist diff --since 1                  # State changes in last hour
homeassist watch sensor.temp --timeout 60 --state 72  # Wait for state
```

### Dashboard Config (YAML)

```yaml
sections:
  - name: Climate
    entities:
      - climate.thermostat
      - sensor.outdoor_temperature
    templates:
      - label: "Net power"
        template: "{{ (states('sensor.solar') | float - states('sensor.grid') | float) | round(1) }}W"
```

Config precedence: `--dashboard` flag > `.homeassist-dashboard.yaml` in cwd > `~/.config/homeassist/dashboard.yaml`

## Config & Automation

```bash
homeassist config check                    # Validate HA config
homeassist config reload all               # Reload automations/scripts/scenes
homeassist automations list                # List with on/off status
homeassist automations trigger <id>
homeassist templates render "{{ states('sensor.temp') }}"
```

## Deployment

```bash
homeassist validate ./packages --check-entities --check-registry || exit 1  # Pre-deploy
homeassist verify --baseline snapshot.json                                   # Post-deploy
```

## Output Modes

Auto-detected: human (terminal), compact (Claude Code), JSON (piped).
Override: `--human`, `--compact`, `--no-compact`.

| Command | Compact format | Savings |
|---------|---------------|---------|
| `stats` | `sys\tv...\tent\ttotal:N\tunavail:N...` (2-3 lines) | ~80% |
| `entities list/search` | TSV: `entity_id\tstate` (capped at 50, `--limit N`) | ~67% |
| `entities get` | Single-line JSON, no context | ~45% |
| `services list <domain>` | `domain.svc(params)` | ~99% |

## Filter Strategy

1. **Domain known?** `--domain sensor`
2. **State known?** `--state unavailable` (exact, not substring)
3. **Keyword?** `search "fountain"`
4. **Regex?** `--pattern "hvac.*power"`
5. **Exact entity?** `get sensor.xyz`

## Pitfalls

- `--pattern unavailable` matches "available" — use `--state unavailable`
- `services list` without domain gives domain names only, not signatures
- Need attributes in list? Add `--no-compact`
- Global flags (`--compact`, `--human`, `--limit`) go BEFORE the subcommand: `homeassist --compact stats`
- Entity lists default to 50 items in compact mode; use `--limit 0` for unlimited or `--limit N`
