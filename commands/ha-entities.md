---
description: Interactive entity exploration — find entities by domain, search, or pattern with progressive detail
---

# HA Entity Explorer

Help the user explore Home Assistant entities interactively:

1. **Start broad** — ask what they're looking for (domain, keyword, or specific entity)
2. **Search** — use the appropriate homeassist command:
   - Known domain: `homeassist entities list --domain <domain>`
   - Keyword: `homeassist entities search "<keyword>"`
   - Regex: `homeassist entities list --pattern "<regex>"`
3. **Drill down** — for entities of interest, get full details with `homeassist entities get <entity_id>`
4. **History** — if they want trends, use `homeassist history get <entity_id> --hours <N>`

Always start with `--compact` mode for initial searches (token-efficient), then use `--no-compact` or `entities get` for full details on specific entities.

Tips:
- Use `--domain` when the user knows the type (sensor, light, climate, etc.)
- Use `search` when they describe something by name ("kitchen", "basement")
- Suggest `history get --compact` for numeric sensors to get min/max/avg
