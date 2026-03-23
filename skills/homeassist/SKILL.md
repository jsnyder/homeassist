---
name: homeassist
description: Use when querying or controlling Home Assistant entities, services, or templates via the homeassist CLI
---

# homeassist CLI

Token-optimized Rust CLI for Home Assistant. Outputs JSON by default, compact TSV when `CLAUDECODE=1` is set or `--compact` flag is passed.

## Quick Reference

| Task | Command |
|------|---------|
| Connection check | `homeassist health` |
| List by domain | `homeassist entities list --domain sensor` |
| Regex filter | `homeassist entities list --pattern "hvac.*power"` |
| Name filter | `homeassist entities list --name "kitchen"` |
| Search id + name | `homeassist entities search "kitchen"` |
| Full entity state | `homeassist entities get sensor.kitchen_temperature` |
| All service domains | `homeassist services list` |
| Domain services | `homeassist services list climate` |
| Call service | `homeassist services call light.turn_on --data '{"entity_id":"light.kitchen"}'` |
| Call with target | `homeassist services call light.turn_on --target '{"area_id":"kitchen"}'` |
| Render template | `homeassist templates render "{{ states('sensor.temp') }}"` |
| State history | `homeassist history get sensor.temp --hours 24` |
| Error log | `homeassist logs errors --tail 50 --pattern "zigbee"` |
| Event timeline | `homeassist logbook get light.kitchen --hours 12` |
| Fire event | `homeassist events fire custom_event --data '{"key":"value"}'` |
| List automations | `homeassist automations list` |
| Trigger automation | `homeassist automations trigger automation.my_auto` |
| List scripts | `homeassist scripts list` |
| Run script | `homeassist scripts run script.my_script` |
| Validate config | `homeassist config check` |
| Reload component | `homeassist config reload automations` |

## Output Modes

Three output modes selected by flags:

- **JSON** (default): `--human` off, `--compact` off. Pretty-printed JSON.
- **Compact**: `--compact` or auto-enabled when `CLAUDECODE=1`. Minimal output optimized for token savings.
- **Human**: `--human` / `-H`. Key-value pairs, readable in terminal.

Override auto-compact with `--no-compact`.

### Compact Format Details

| Command | Compact format | Token savings |
|---------|---------------|--------------|
| `entities list/search` | TSV: `entity_id\tstate` per line | ~67% |
| `entities get` | Single-line JSON, no context/timestamps | ~45% |
| `services list <domain>` | `domain.svc(param1, param2)` per line | ~99% |
| `history get` (numeric) | `min=X\tmax=X\tavg=X\tsamples=N` | ~90% |
| `logs errors` | Plain text lines | ~80% |

## Decision Tree: Finding Entities

```
Do you know the domain? (sensor, light, climate, etc.)
├── Yes → homeassist entities list --domain <domain>
└── No
    ├── Have a keyword? → homeassist entities search "<keyword>"
    ├── Need regex? → homeassist entities list --pattern "<regex>"
    └── Know exact ID? → homeassist entities get <entity_id>
```

## Auth

Resolved in precedence order:
1. `--url` / `--token` flags
2. `HA_URL` / `HA_TOKEN` environment variables
3. `~/.ha_url` / `~/.ha_token` files (must be `chmod 600`)

## Common Mistakes

| Mistake | Better approach |
|---------|---------------|
| `search` when domain is known | `list --domain X` is faster and more precise |
| Forgetting `--no-compact` | Use it when you need attribute values from a list |
| `services list` without domain | Returns only domain names, not service signatures |
| `history get` without `--hours` | Defaults to 24h — specify a smaller window for less data |
