---
name: homeassist
description: Use when querying or controlling Home Assistant — entities, services, templates, logs, automations, system health. Also for pre-deploy YAML validation and post-deploy verification.
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

## System Monitoring

```bash
homeassist health                          # Connection + version
homeassist inspect                         # Unavailable/unknown audit, domain summary
homeassist logs errors --tail 10           # System log via WebSocket (all installs)
homeassist logs errors --pattern "zigbee"  # Filter by regex
homeassist diff --since 1                  # State changes in last hour
homeassist watch sensor.temp --timeout 60 --state 72  # Wait for state
```

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
homeassist validate ./packages --check-entities || exit 1  # Pre-deploy
homeassist verify --baseline snapshot.json                  # Post-deploy
```

## Output Modes

Auto-detected: human (terminal), compact (Claude Code), JSON (piped).
Override: `--human`, `--compact`, `--no-compact`.

| Command | Compact format | Savings |
|---------|---------------|---------|
| `entities list/search` | TSV: `entity_id\tstate` | ~67% |
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
