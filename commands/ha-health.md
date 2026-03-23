---
description: Comprehensive Home Assistant health check — connection, version, entity counts, error log summary
---

# HA Health Check

Run a comprehensive health check against the Home Assistant instance:

1. **Connection test** — run `homeassist health` to verify connectivity and get version
2. **Entity summary** — run `homeassist entities list --compact` and count entities by domain (use `wc -l` or count lines by domain prefix)
3. **Error check** — run `homeassist logs errors --tail 20` to check for recent errors
4. **Automation status** — run `homeassist automations list --compact` and report how many are on vs off

Present results as a concise summary table. Flag any:
- Automations in "off" state (may be intentionally disabled — note but don't alarm)
- Recent errors in the log (last 20 lines)
- Connection failures

Example output format:
```
HA Health: connected (v2024.3.1) @ http://homeassistant.local:8123
Entities: 247 (sensor: 142, light: 28, switch: 19, ...)
Automations: 34 on, 3 off
Recent errors: 2 in last 20 lines
```
