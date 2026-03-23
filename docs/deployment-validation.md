# Deployment Validation

homeassist replaces ~1,200 lines of shell-based deployment validation
(`deploy-unified.sh`) with a compiled Rust binary that runs 40-170x faster.

## Performance Comparison

Benchmarked against `deploy-unified.sh` (1,937 lines of bash) on a real
production config: 157 YAML package files, 10,258 entities, 658 automations.

| Step | deploy-unified.sh | homeassist | Speedup |
|------|------------------:|-----------:|--------:|
| Local YAML validation | ~30s | **0.12s** | ~250x |
| Validation + entity ref checks | ~35s | **0.25s** | ~140x |
| Full validate + verify | ~42s | **1.3s** | ~32x |

### Why the difference

`deploy-unified.sh` spawns hundreds of subprocesses per run — `grep`, `sed`,
`awk`, `python3`, and `curl` calls for each file and each validation check.
Each subprocess has shell startup overhead, and entity checks make individual
HTTP requests per entity.

homeassist does everything in a single compiled binary:
- YAML parsing via `serde_yaml` (no subprocess per file)
- All validation checks in one pass over the parsed data
- Entity reference checks use a single bulk `/api/states` call (one HTTP
  request for all 10,258 entities) then matches locally
- Verify uses `tokio::join!` to run config check and state fetch concurrently

## What Each Tool Validates

### Checks that homeassist replaces

| Validation | deploy-unified.sh function | homeassist equivalent |
|------------|---------------------------|----------------------|
| YAML syntax | `validate_yaml_syntax` | `validate` |
| Template/Jinja format | `validate_template_format` | `validate` (jinja balance) |
| Sensor platforms | `validate_sensor_platforms` | `validate` |
| Common errors | `validate_common_errors` | `validate` |
| Duplicate entities | `check_duplicate_entity_definitions` | `validate` |
| File cruft | `check_file_cruft` | `validate` |
| Package exclusions | `validate_package_exclusions` | `validate` |
| Automation syntax | `validate_automation_syntax` | `validate` |
| Entity references | `validate_template_entities` | `validate --check-entities` |
| Issue baseline | `compare_issues_to_baseline` | `verify --baseline` |
| Config validity | `mcp_check_config` | `config check` / `verify` |
| Post-deploy health | `verify_deployment` | `verify` |

### Checks not yet in homeassist

| Validation | Notes |
|------------|-------|
| `validate_entity_registry` | Entity registry consistency checks |
| Circular reference detection | External Python script in deploy-unified |
| Pyscript test runner | Separate test infrastructure, not in scope |

### Checks only in homeassist

| Validation | Notes |
|------------|-------|
| `--state` entity filter | Exact match (avoids regex substring issues) |
| WebSocket system logs | Works on HAOS where REST `/api/error_log` returns 404 |
| Automation state audit | on/off counts with baseline delta |
| Human terminal UI | Styled output with spinners, colors, aligned tables |

## Deployment Workflow

### Minimal (today)

```bash
# Pre-deploy: validate local config files
homeassist validate ./packages --check-entities || exit 1

# Deploy via your existing method
rsync -av packages/ ha-host:/config/packages/

# Post-deploy: verify system health
homeassist verify --baseline snapshot.json
```

### What deploy-unified.sh still provides

The transport layer — backup creation, rsync file sync, HA restart/rollback —
is not yet in homeassist. These are the Tier 2 features on the roadmap:

- `create_backup` — SSH backup of remote config
- `deploy_files` — rsync with exclusion patterns
- `restart_homeassistant` — restart via SSH or MCP
- `rollback_deployment` — restore from backup on failure
- `deploy_node_red_flows` — Node-RED specific deployment

A thin wrapper script (~50 lines) can bridge the gap:

```bash
#!/bin/bash
set -euo pipefail

# Validate locally
homeassist validate ./packages --check-entities || exit 1

# Save pre-deploy baseline
homeassist --compact verify > /tmp/baseline.json 2>&1

# Backup + deploy (from existing script)
ssh root@ha-host "cp -r /config/packages /config/backups/deploy-$(date +%Y%m%d_%H%M%S)"
rsync -av --delete packages/ root@ha-host:/config/packages/

# Reload HA config
homeassist config reload all

# Verify post-deploy
homeassist verify --baseline /tmp/baseline.json || {
    echo "Verification failed — consider rollback"
    exit 1
}
```

## Migration Path

1. **Now**: Use `homeassist validate` and `homeassist verify` alongside
   deploy-unified.sh to compare results
2. **Next**: Replace the validation steps in deploy-unified.sh with homeassist
   calls (keeps transport layer)
3. **Future**: Tier 2 deploy orchestration in homeassist replaces the full script
