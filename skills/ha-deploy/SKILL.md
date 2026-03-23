---
name: ha-deploy
description: Use when deploying Home Assistant config, validating YAML packages, or verifying system health after changes. Replaces shell-based validation with 40-170x faster compiled checks.
---

# HA Deployment Validation

Pre-deploy validation and post-deploy verification for Home Assistant config packages.

## Pre-Deploy: Validate

Run before deploying config to HA:

```bash
homeassist validate ./packages                    # YAML syntax + structure
homeassist validate ./packages --check-entities   # + verify entity refs against live HA
homeassist validate ./packages --check-registry   # + detect orphaned entity registry entries
```

### What it checks
- YAML syntax and structure
- Package exclusion (blocked top-level keys like recorder, logger in packages)
- Automation syntax (trigger/action required, alias recommended, blueprint-aware)
- Sensor/binary_sensor platform validation
- Circular reference detection (template sensors referencing themselves)
- Jinja2 template balance
- Duplicate entity IDs across files
- Common errors (tabs, bare unavailable, None thresholds)
- File cruft (backup files, temp files)
- Entity references exist in live HA (`--check-entities`)
- Orphaned entity registry entries (`--check-registry`, via WebSocket)

### Exit codes
- `0` — no errors (warnings are non-blocking)
- `1` — errors found, do not deploy

## Post-Deploy: Verify

Run after deploying and reloading HA:

```bash
homeassist verify                              # Health check
homeassist verify --baseline snapshot.json     # Compare against baseline
```

### What it checks
- HA configuration validity
- Entity counts (total, unavailable, unknown)
- Automation states (on/off)
- Baseline delta (did unavailable count increase?)

## Full Deploy Workflow

```bash
# 1. Validate locally
homeassist validate ./packages --check-entities --check-registry || exit 1

# 2. Deploy (rsync, git pull, etc.)
rsync -av packages/ root@ha-host:/config/packages/

# 3. Reload
homeassist config reload all

# 4. Verify
homeassist verify --baseline snapshot.json
```

## Performance

| Step | Shell script | homeassist |
|------|------------:|----------:|
| YAML validation (157 files) | ~30s | **0.12s** |
| + entity ref checks | ~35s | **0.25s** |
| Full validate + verify | ~42s | **1.3s** |
