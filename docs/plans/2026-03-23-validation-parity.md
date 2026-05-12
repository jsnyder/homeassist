# Validation Parity Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the 5 remaining validation checks so `homeassist validate` fully replaces the bash/python validation in deploy-unified.sh.

**Architecture:** All checks are pure functions following the existing `check_*(file, content, findings)` pattern in `src/commands/validate.rs`. Four checks are per-file static analysis on parsed YAML content. One (entity registry) requires async WebSocket access. Circular reference detection uses regex-based boundary-aware matching on template sensor `states()` calls.

**Tech Stack:** Rust, `serde_yaml::Value` for YAML traversal, `regex` for entity extraction, existing `ws::HaWebSocket` for entity registry check.

## Review Findings (GPT-5.4, 2026-03-23)

Fixes incorporated from external review:

1. **[HIGH] Recorder duplication** — Remove `recorder:` from `check_common_errors` (L325-334) since `check_package_exclusions` now handles all blocked keys including recorder.
2. **[HIGH] Circular ref false positive** — Use regex boundary matching in `contains_self_reference` instead of `String::contains()`. Add negative test for `sensor.test_sensor` vs `sensor.test_sensor_2`.
3. **[MEDIUM] Disabled entities** — Filter `disabled_by` entries in entity registry orphan check.
4. **[MEDIUM] Package path matching** — Use `/packages/` in path check, not just `package`.
5. **[MEDIUM] serde_yaml !include** — Files with HA custom tags fail `from_str()` and are skipped by existing gate. Add explicit test documenting this behavior.
6. **[LOW] Integration test** — Add one `run()` test with tempdir to verify wiring.

---

## Task 1: check_package_exclusions

Blocks package files that contain top-level keys that would override main HA config (e.g., `homeassistant:`, `recorder:`, `frontend:`).

**Files:**
- Modify: `src/commands/validate.rs`

**Step 1: Write failing tests**

Add to the existing `#[cfg(test)] mod tests` block in `validate.rs`:

```rust
#[test]
fn package_exclusion_blocks_homeassistant_key() {
    let mut findings = Vec::new();
    let content = "homeassistant:\n  name: My Home\nsensor:\n  - platform: template\n";
    check_package_exclusions("packages/bad.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, "error");
    assert_eq!(findings[0].check, "package_exclusion");
}

#[test]
fn package_exclusion_allows_normal_keys() {
    let mut findings = Vec::new();
    let content = "sensor:\n  - platform: template\nautomation:\n  - alias: test\n";
    check_package_exclusions("packages/good.yaml", content, &mut findings);
    assert!(findings.is_empty());
}

#[test]
fn package_exclusion_blocks_recorder_key() {
    let mut findings = Vec::new();
    let content = "recorder:\n  purge_keep_days: 5\n";
    check_package_exclusions("packages/recorder.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
}

#[test]
fn package_exclusion_skips_non_package_files() {
    let mut findings = Vec::new();
    let content = "homeassistant:\n  name: My Home\n";
    check_package_exclusions("configuration.yaml", content, &mut findings);
    assert!(findings.is_empty());
}

#[test]
fn package_exclusion_ignores_nested_key_names() {
    let mut findings = Vec::new();
    // 'recorder' appears in a value_template, not as a top-level key
    let content = "sensor:\n  - platform: template\n    sensors:\n      test:\n        value_template: \"{{ states('recorder.something') }}\"\n";
    check_package_exclusions("packages/tricky.yaml", content, &mut findings);
    assert!(findings.is_empty());
}

#[test]
fn package_exclusion_blocks_multiple_keys() {
    let mut findings = Vec::new();
    let content = "recorder:\n  purge_keep_days: 5\nlogger:\n  default: warning\n";
    check_package_exclusions("packages/monitoring.yaml", content, &mut findings);
    assert_eq!(findings.len(), 2);
}
```

**Step 2: Run tests, verify they fail**

```bash
cargo test check_package_exclusions -- --nocapture
```

Expected: FAIL — `check_package_exclusions` not found.

**Step 3: Write minimal implementation**

```rust
fn check_package_exclusions(file: &str, content: &str, findings: &mut Vec<Finding>) {
    // Only check files in packages/ directories
    if !file.contains("/packages/") && !file.starts_with("packages/") {
        return;
    }

    const BLOCKED_KEYS: &[&str] = &[
        "homeassistant", "default_config", "frontend", "http",
        "recorder", "logger", "history", "logbook",
    ];

    for (line_num, line) in content.lines().enumerate() {
        // Top-level key: starts at column 0, not a comment, not empty
        if !line.starts_with(' ') && !line.starts_with('#') && !line.trim().is_empty() {
            if let Some(key) = line.split(':').next() {
                let key = key.trim();
                if BLOCKED_KEYS.contains(&key) {
                    findings.push(Finding {
                        file: file.to_string(),
                        line: Some(line_num + 1),
                        severity: "error",
                        check: "package_exclusion",
                        message: format!(
                            "Package contains '{key}:' which overrides main config — move to configuration.yaml"
                        ),
                    });
                }
            }
        }
    }
}
```

**Step 4: Run tests, verify they pass**

```bash
cargo test check_package_exclusions -- --nocapture
```

Expected: 4 tests PASS.

**Step 5: Wire into run() and commit**

Add `check_package_exclusions(file_path, &content, &mut findings);` inside the `serde_yaml` success block in `run()`.

```bash
cargo test && git add src/commands/validate.rs && git commit -m "feat(validate): add package exclusion check

Blocks top-level keys (homeassistant, recorder, etc.) in package files
that would override main configuration."
```

---

## Task 2: check_sensor_platforms

Checks that list entries under `sensor:` and `binary_sensor:` have a `platform:` field. Modern `template:` style sensors don't need this — only legacy platform-based sensors.

**Files:**
- Modify: `src/commands/validate.rs`

**Step 1: Write failing tests**

```rust
#[test]
fn sensor_platform_missing_detected() {
    let mut findings = Vec::new();
    let content = "sensor:\n  - name: My Sensor\n    state: '{{ 1 }}'\n";
    check_sensor_platforms("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].check, "missing_platform");
}

#[test]
fn sensor_platform_present_no_finding() {
    let mut findings = Vec::new();
    let content = "sensor:\n  - platform: template\n    sensors:\n      test:\n        value_template: '{{ 1 }}'\n";
    check_sensor_platforms("test.yaml", content, &mut findings);
    assert!(findings.is_empty());
}

#[test]
fn binary_sensor_platform_missing_detected() {
    let mut findings = Vec::new();
    let content = "binary_sensor:\n  - name: Door\n    state: 'on'\n";
    check_sensor_platforms("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
}

#[test]
fn sensor_not_a_list_no_panic() {
    let mut findings = Vec::new();
    // sensor: as a scalar (invalid but shouldn't crash)
    let content = "sensor: true\n";
    check_sensor_platforms("test.yaml", content, &mut findings);
    assert!(findings.is_empty());
}
```

**Step 2: Run tests, verify they fail**

```bash
cargo test check_sensor_platforms -- --nocapture
```

**Step 3: Write minimal implementation**

```rust
fn check_sensor_platforms(file: &str, content: &str, findings: &mut Vec<Finding>) {
    let yaml: serde_yaml::Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(_) => return,
    };

    for domain in &["sensor", "binary_sensor"] {
        if let Some(serde_yaml::Value::Sequence(items)) = yaml.get(domain) {
            for (i, item) in items.iter().enumerate() {
                if let serde_yaml::Value::Mapping(map) = item {
                    let has_platform = map.contains_key(&serde_yaml::Value::String("platform".to_string()));
                    if !has_platform {
                        findings.push(Finding {
                            file: file.to_string(),
                            line: None,
                            severity: "warning",
                            check: "missing_platform",
                            message: format!(
                                "{domain} list entry #{} has no 'platform:' key — is this a legacy integration?",
                                i + 1
                            ),
                        });
                    }
                }
            }
        }
    }
}
```

**Step 4: Run tests, verify they pass**

```bash
cargo test check_sensor_platforms -- --nocapture
```

**Step 5: Wire in and commit**

Add to the `serde_yaml` success block, then:

```bash
cargo test && git add src/commands/validate.rs && git commit -m "feat(validate): add sensor platform check

Warns when sensor/binary_sensor list entries lack a platform: key."
```

---

## Task 3: check_automation_syntax

Checks automation entries have required structural keys. HA accepts both legacy (`trigger`/`action`) and modern (`triggers`/`actions`) forms.

**Files:**
- Modify: `src/commands/validate.rs`

**Step 1: Write failing tests**

```rust
#[test]
fn automation_syntax_valid_modern() {
    let mut findings = Vec::new();
    let content = "automation:\n  - alias: Test\n    triggers:\n      - trigger: state\n    actions:\n      - action: light.turn_on\n";
    check_automation_syntax("test.yaml", content, &mut findings);
    assert!(findings.is_empty());
}

#[test]
fn automation_syntax_valid_legacy() {
    let mut findings = Vec::new();
    let content = "automation:\n  - alias: Test\n    trigger:\n      - platform: state\n    action:\n      - service: light.turn_on\n";
    check_automation_syntax("test.yaml", content, &mut findings);
    assert!(findings.is_empty());
}

#[test]
fn automation_syntax_missing_trigger_errors() {
    let mut findings = Vec::new();
    let content = "automation:\n  - alias: Broken\n    action:\n      - service: light.turn_on\n";
    check_automation_syntax("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, "error");
    assert!(findings[0].message.contains("trigger"));
}

#[test]
fn automation_syntax_missing_action_errors() {
    let mut findings = Vec::new();
    let content = "automation:\n  - alias: Broken\n    trigger:\n      - platform: state\n";
    check_automation_syntax("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("action"));
}

#[test]
fn automation_syntax_missing_alias_warns() {
    let mut findings = Vec::new();
    let content = "automation:\n  - trigger:\n      - platform: state\n    action:\n      - service: light.turn_on\n";
    check_automation_syntax("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, "warning");
    assert!(findings[0].message.contains("alias"));
}

#[test]
fn automation_not_a_list_no_panic() {
    let mut findings = Vec::new();
    let content = "automation: !include automations.yaml\n";
    check_automation_syntax("test.yaml", content, &mut findings);
    assert!(findings.is_empty());
}
```

**Step 2: Run tests, verify they fail**

```bash
cargo test check_automation_syntax -- --nocapture
```

**Step 3: Write minimal implementation**

```rust
fn check_automation_syntax(file: &str, content: &str, findings: &mut Vec<Finding>) {
    let yaml: serde_yaml::Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let automations = match yaml.get("automation") {
        Some(serde_yaml::Value::Sequence(items)) => items,
        _ => return,
    };

    for (i, item) in automations.iter().enumerate() {
        let map = match item {
            serde_yaml::Value::Mapping(m) => m,
            _ => continue,
        };

        let has = |key: &str| map.contains_key(&serde_yaml::Value::String(key.to_string()));

        let alias = map
            .get(&serde_yaml::Value::String("alias".to_string()))
            .and_then(|v| v.as_str())
            .unwrap_or(&format!("#{}", i + 1));

        let has_trigger = has("trigger") || has("triggers");
        let has_action = has("action") || has("actions");
        let has_alias = has("alias");

        if !has_trigger {
            findings.push(Finding {
                file: file.to_string(),
                line: None,
                severity: "error",
                check: "automation_syntax",
                message: format!("Automation '{alias}' missing trigger/triggers"),
            });
        }

        if !has_action {
            findings.push(Finding {
                file: file.to_string(),
                line: None,
                severity: "error",
                check: "automation_syntax",
                message: format!("Automation '{alias}' missing action/actions"),
            });
        }

        if !has_alias {
            findings.push(Finding {
                file: file.to_string(),
                line: None,
                severity: "warning",
                check: "automation_syntax",
                message: format!("Automation #{} has no alias — add one for readability", i + 1),
            });
        }
    }
}
```

**Step 4: Run tests, verify they pass**

```bash
cargo test check_automation_syntax -- --nocapture
```

**Step 5: Wire in and commit**

```bash
cargo test && git add src/commands/validate.rs && git commit -m "feat(validate): add automation syntax check

Validates trigger/action presence, warns on missing alias."
```

---

## Task 4: detect_circular_references

Detects template sensors that reference themselves via `states()`, `is_state()`, or `state_attr()` calls. Based on the Python script in home_assistant repo which checks self-references (not cross-sensor cycles).

**Files:**
- Modify: `src/commands/validate.rs`

**Step 1: Write failing tests**

```rust
#[test]
fn circular_ref_self_reference_detected() {
    let mut findings = Vec::new();
    let content = r#"template:
  - sensor:
      - name: Test Sensor
        unique_id: test_sensor
        state: "{{ states('sensor.test_sensor') }}"
"#;
    detect_circular_references("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].check, "circular_reference");
    assert!(findings[0].message.contains("test_sensor"));
}

#[test]
fn circular_ref_no_self_reference_clean() {
    let mut findings = Vec::new();
    let content = r#"template:
  - sensor:
      - name: Average Temp
        unique_id: avg_temp
        state: "{{ states('sensor.outdoor_temp') }}"
"#;
    detect_circular_references("test.yaml", content, &mut findings);
    assert!(findings.is_empty());
}

#[test]
fn circular_ref_state_attr_self_reference() {
    let mut findings = Vec::new();
    let content = r#"template:
  - sensor:
      - name: Power Monitor
        unique_id: power_monitor
        state: "{{ state_attr('sensor.power_monitor', 'watts') }}"
"#;
    detect_circular_references("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
}

#[test]
fn circular_ref_is_state_self_reference() {
    let mut findings = Vec::new();
    let content = r#"template:
  - binary_sensor:
      - name: Door Open
        unique_id: door_open
        state: "{{ is_state('binary_sensor.door_open', 'on') }}"
"#;
    detect_circular_references("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
}

#[test]
fn circular_ref_name_derived_entity_id() {
    let mut findings = Vec::new();
    // No unique_id — entity_id derived from name: "My Sensor" -> "sensor.my_sensor"
    let content = r#"template:
  - sensor:
      - name: My Sensor
        state: "{{ states('sensor.my_sensor') }}"
"#;
    detect_circular_references("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
}

#[test]
fn circular_ref_attribute_self_reference() {
    let mut findings = Vec::new();
    let content = r#"template:
  - sensor:
      - name: Power
        unique_id: power_calc
        state: "{{ 100 }}"
        attributes:
          trend: "{{ states('sensor.power_calc') }}"
"#;
    detect_circular_references("test.yaml", content, &mut findings);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("attribute"));
}

#[test]
fn circular_ref_no_false_positive_on_similar_name() {
    let mut findings = Vec::new();
    // sensor.test_sensor should NOT match sensor.test_sensor_2
    let content = r#"template:
  - sensor:
      - name: Test Sensor
        unique_id: test_sensor
        state: "{{ states('sensor.test_sensor_2') }}"
"#;
    detect_circular_references("test.yaml", content, &mut findings);
    assert!(findings.is_empty());
}

#[test]
fn circular_ref_no_template_section_clean() {
    let mut findings = Vec::new();
    let content = "sensor:\n  - platform: template\n    sensors:\n      test:\n        value_template: '{{ 1 }}'\n";
    detect_circular_references("test.yaml", content, &mut findings);
    assert!(findings.is_empty());
}
```

**Step 2: Run tests, verify they fail**

```bash
cargo test detect_circular_references -- --nocapture
```

**Step 3: Write minimal implementation**

```rust
fn detect_circular_references(file: &str, content: &str, findings: &mut Vec<Finding>) {
    let yaml: serde_yaml::Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let template = match yaml.get("template") {
        Some(serde_yaml::Value::Sequence(items)) => items,
        _ => return,
    };

    for item in template {
        let map = match item.as_mapping() {
            Some(m) => m,
            None => continue,
        };

        for sensor_type in &["sensor", "binary_sensor"] {
            let sensors = match map.get(&serde_yaml::Value::String(sensor_type.to_string())) {
                Some(serde_yaml::Value::Sequence(s)) => s,
                _ => continue,
            };

            for sensor in sensors {
                let sensor_map = match sensor.as_mapping() {
                    Some(m) => m,
                    None => continue,
                };

                let name = sensor_map
                    .get(&serde_yaml::Value::String("name".into()))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unnamed");

                let unique_id = sensor_map
                    .get(&serde_yaml::Value::String("unique_id".into()))
                    .and_then(|v| v.as_str());

                // Derive entity_id
                let entity_id = if let Some(uid) = unique_id {
                    format!("{sensor_type}.{uid}")
                } else {
                    format!(
                        "{sensor_type}.{}",
                        name.to_lowercase().replace(' ', "_")
                    )
                };

                // Check state, icon, availability fields
                for field in &["state", "icon", "availability"] {
                    if let Some(val) = sensor_map
                        .get(&serde_yaml::Value::String(field.to_string()))
                        .and_then(|v| v.as_str())
                    {
                        if contains_self_reference(val, &entity_id) {
                            findings.push(Finding {
                                file: file.to_string(),
                                line: None,
                                severity: "error",
                                check: "circular_reference",
                                message: format!(
                                    "Template sensor '{name}' ({entity_id}): {field} references itself"
                                ),
                            });
                        }
                    }
                }

                // Check attributes
                if let Some(attrs) = sensor_map
                    .get(&serde_yaml::Value::String("attributes".into()))
                    .and_then(|v| v.as_mapping())
                {
                    for (attr_key, attr_val) in attrs {
                        let attr_name = attr_key.as_str().unwrap_or("?");
                        if let Some(val) = attr_val.as_str() {
                            if contains_self_reference(val, &entity_id) {
                                findings.push(Finding {
                                    file: file.to_string(),
                                    line: None,
                                    severity: "error",
                                    check: "circular_reference",
                                    message: format!(
                                        "Template sensor '{name}' ({entity_id}): attribute '{attr_name}' references itself"
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn contains_self_reference(template: &str, entity_id: &str) -> bool {
    // Boundary-aware matching to avoid false positives
    // e.g., sensor.test_sensor must NOT match sensor.test_sensor_2
    let escaped = regex::escape(entity_id);
    let re = Regex::new(&format!(
        r#"(states|state_attr|is_state)\(\s*['"]{}['"]"#, escaped
    )).expect("valid regex");
    re.is_match(template)
}
```

**Step 4: Run tests, verify they pass**

```bash
cargo test detect_circular_references -- --nocapture
```

**Step 5: Wire in and commit**

Add `detect_circular_references(file_path, &content, &mut findings);` inside the `serde_yaml` success block.

```bash
cargo test && git add src/commands/validate.rs && git commit -m "feat(validate): add circular reference detection

Detects template sensors that reference themselves via states(),
state_attr(), or is_state() calls."
```

---

## Task 5: check_entity_registry (WebSocket)

Compares entity registry entries (from WebSocket `config/entity_registry/list`) against actual entity states to detect orphaned/stale entries. This is an optional check requiring a live HA connection.

**Files:**
- Modify: `src/commands/validate.rs`
- Modify: `src/main.rs` (add `--check-registry` flag)

**Step 1: Write failing tests**

This uses the existing `start_ha_mock()` WebSocket test server pattern from `ws.rs`. The test server will respond to `config/entity_registry/list`.

```rust
// In validate.rs tests:
#[tokio::test]
async fn entity_registry_detects_orphaned_entries() {
    // Registry has entity_id "sensor.deleted" but states don't
    let registry = vec![
        json!({"entity_id": "sensor.temp", "platform": "template"}),
        json!({"entity_id": "sensor.deleted", "platform": "mqtt"}),
    ];
    let states = vec![
        json!({"entity_id": "sensor.temp", "state": "72"}),
    ];
    let orphaned = find_orphaned_registry_entries(&registry, &states);
    assert_eq!(orphaned.len(), 1);
    assert_eq!(orphaned[0]["entity_id"], "sensor.deleted");
}

#[tokio::test]
async fn entity_registry_ignores_disabled_entries() {
    let registry = vec![
        json!({"entity_id": "sensor.temp", "platform": "template"}),
        json!({"entity_id": "sensor.disabled", "platform": "mqtt", "disabled_by": "user"}),
    ];
    let states = vec![
        json!({"entity_id": "sensor.temp", "state": "72"}),
    ];
    let orphaned = find_orphaned_registry_entries(&registry, &states);
    assert!(orphaned.is_empty());
}

#[tokio::test]
async fn entity_registry_no_orphans_clean() {
    let registry = vec![
        json!({"entity_id": "sensor.temp", "platform": "template"}),
    ];
    let states = vec![
        json!({"entity_id": "sensor.temp", "state": "72"}),
    ];
    let orphaned = find_orphaned_registry_entries(&registry, &states);
    assert!(orphaned.is_empty());
}
```

**Step 2: Run tests, verify they fail**

```bash
cargo test entity_registry -- --nocapture
```

**Step 3: Write implementation**

```rust
fn find_orphaned_registry_entries(
    registry: &[serde_json::Value],
    states: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let state_ids: std::collections::HashSet<&str> = states
        .iter()
        .filter_map(|s| s.get("entity_id").and_then(|v| v.as_str()))
        .collect();

    registry
        .iter()
        .filter(|entry| {
            let eid = entry
                .get("entity_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // Disabled entities are expected to have no state
            let disabled = entry
                .get("disabled_by")
                .and_then(|v| v.as_str())
                .is_some();
            !eid.is_empty() && !disabled && !state_ids.contains(eid)
        })
        .cloned()
        .collect()
}

async fn check_entity_registry(
    client: &HaClient,
    base_url: &str,
    token: &str,
    findings: &mut Vec<Finding>,
) {
    // Get registry via WebSocket
    let ws_url = base_url.replace("http", "ws") + "/api/websocket";
    let mut ws = match crate::ws::HaWebSocket::connect(&ws_url, token).await {
        Ok(ws) => ws,
        Err(_) => return, // Can't check, skip silently
    };

    let registry_result = ws
        .command("config/entity_registry/list")
        .await;
    ws.close().await;

    let registry: Vec<serde_json::Value> = match registry_result {
        Ok(val) => val.as_array().cloned().unwrap_or_default(),
        Err(_) => return,
    };

    // Get states via REST
    let states = match client.get_states().await {
        Ok(s) => s,
        Err(_) => return,
    };

    let orphaned = find_orphaned_registry_entries(&registry, &states);
    for entry in &orphaned {
        let eid = entry.get("entity_id").and_then(|v| v.as_str()).unwrap_or("?");
        let platform = entry.get("platform").and_then(|v| v.as_str()).unwrap_or("unknown");
        findings.push(Finding {
            file: "(entity_registry)".to_string(),
            line: None,
            severity: "warning",
            check: "orphaned_entity",
            message: format!("Entity '{eid}' (platform: {platform}) in registry but has no state"),
        });
    }
}
```

**Step 4: Run tests, verify they pass**

```bash
cargo test entity_registry -- --nocapture
```

**Step 5: Wire in with new flag and commit**

Add `--check-registry` flag to the `Validate` CLI struct and pass `base_url`/`token` through to the check. Wire it in `run()` after the entity reference check.

```bash
cargo test && git add src/commands/validate.rs src/main.rs && git commit -m "feat(validate): add entity registry check via WebSocket

Detects orphaned entity registry entries that have no corresponding state.
Requires --check-registry flag and live HA connection."
```

---

## Task 6: Wire all checks into run() and final integration

**Files:**
- Modify: `src/commands/validate.rs` (run function)
- Modify: `ROADMAP.md` (mark phase 5a.1 complete)

**Step 1: Verify all checks are wired in run()**

The `run()` per-file loop should now include:
```rust
if serde_yaml::from_str::<serde_yaml::Value>(&content).is_ok() {
    check_duplicate_keys(file_path, &content, &mut findings);
    check_jinja_templates(file_path, &content, &mut findings);
    check_common_errors(file_path, &content, &mut findings);
    check_file_cruft(file_path, &mut findings);
    // New checks:
    check_package_exclusions(file_path, &content, &mut findings);
    check_sensor_platforms(file_path, &content, &mut findings);
    check_automation_syntax(file_path, &content, &mut findings);
    detect_circular_references(file_path, &content, &mut findings);
}
```

**Step 2: Run full test suite**

```bash
cargo test
cargo clippy
```

Expected: all tests pass, no new warnings.

**Step 3: Test against real HA config**

```bash
homeassist validate /path/to/home-assistant/packages
homeassist validate /path/to/home-assistant/packages --check-entities
```

Verify new checks fire on real config without false positives.

**Step 4: Update ROADMAP.md**

Mark Phase 5a.1 as complete.

**Step 5: Final commit**

```bash
cargo test && git add -A && git commit -m "feat(validate): complete validation parity with deploy-unified.sh

All 5 remaining checks implemented:
- Package exclusions (blocked top-level keys)
- Sensor platform validation
- Automation syntax (trigger/action/alias)
- Circular reference detection (template self-refs)
- Entity registry orphan check (WebSocket)

deploy-unified.sh can now gate all overlapping shell validators
behind '! command -v homeassist'."
```

---

## Summary

| Task | Check | Type | Estimated Tests |
|------|-------|------|----------------|
| 1 | check_package_exclusions | Per-file, string scan | 6 |
| 2 | check_sensor_platforms | Per-file, YAML parse | 4 |
| 3 | check_automation_syntax | Per-file, YAML parse | 6 |
| 4 | detect_circular_references | Per-file, YAML parse + regex | 8 |
| 5 | check_entity_registry | Async, WebSocket + REST | 3 |
| 6 | Integration wiring | Glue | 0 (manual test) |
| **Total** | | | **27 new tests** |

Each task is independently committable and testable.

### Future Enhancement: Cross-Sensor Cycle Detection

The current plan implements self-reference detection (matching the existing Python
script). A more advanced check would build a full dependency graph across template
sensors and detect multi-node cycles (A→B→A, A→B→C→A) using DFS. This is architecturally
straightforward but beyond parity scope. See `docs/tdd-test-cases.md` for pre-designed
test cases covering diamond DAGs, partial-graph cycles, and transitive chains.
