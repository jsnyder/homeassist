---
description: Validate, deploy, and verify Home Assistant configuration changes
---

# HA Deploy Workflow

Safely deploy Home Assistant configuration changes with validation:

## Pre-deploy

1. **Check config** — `homeassist config check`
   - If invalid, stop and show the error. Do not proceed.
2. **Review changes** — show what files were modified (use git diff if in a repo)

## Deploy

3. **Reload components** — based on what changed:
   - Automations modified: `homeassist config reload automations`
   - Scripts modified: `homeassist config reload scripts`
   - Scenes modified: `homeassist config reload scenes`
   - Multiple: `homeassist config reload all`
   - If changes require a full restart (core config, integrations), warn the user

## Post-deploy

4. **Verify** — check that reloaded entities are healthy:
   - `homeassist health` — confirm still connected
   - For automations: `homeassist automations list --compact` — check states
   - For specific entities: `homeassist entities get <entity_id>` — confirm not "unavailable"
5. **Test** — if automation tests exist, run them:
   - `homeassist services call pyscript.run_<feature>_tests`
   - Check counters for pass/fail

## Rollback guidance

If verification fails:
- Revert the config changes (git checkout or manual)
- Reload again
- Verify the rollback succeeded
