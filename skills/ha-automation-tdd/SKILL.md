---
name: ha-automation-tdd
description: Use when implementing Home Assistant automations - test YAML automations using pyscript as a test harness with script-based architecture
---

# Home Assistant Automation TDD

Test YAML automations using pyscript as a test harness.

## Core Principles

1. **Scripts contain testable logic** — automations are thin trigger wrappers
2. **Never use `automation.trigger`** — it doesn't populate `trigger.*` variables
3. **Explicit inputs via `fields:`** — no global state reads inside scripts
4. **Every branch emits observation** — test mode observation for verification

## Architecture

```
Script (testable logic)    ← receives explicit inputs via fields:
    ↑
Automation (thin wrapper)  ← wires trigger → script with data mapping
    ↑
Trigger (state change, time, event)
```

## When to Use

Use TDD for: conditional logic, state machines, HVAC/presence control, long-lived automations.

Skip for: single-action automations, UI-created, temporary/experimental.

## Implementation Steps

### 1. Write the Script (testable logic)

```yaml
script:
  my_automation_logic:
    alias: "My Automation Logic"
    fields:
      sensor_state: { description: "Current sensor state" }
      threshold: { description: "Threshold value" }
    sequence:
      - choose:
          - conditions:
              - condition: template
                value_template: "{{ sensor_state == 'on' and threshold > 50 }}"
            sequence:
              - service: light.turn_on
                target: { entity_id: light.living_room }
              - if:
                  - condition: state
                    entity_id: input_boolean.pyscript_test_mode
                    state: "on"
                then:
                  - service: input_text.set_value
                    target: { entity_id: input_text.aut_test_observed }
                    data: { value: "branch_1_executed" }
        default:
          - if:
              - condition: state
                entity_id: input_boolean.pyscript_test_mode
                state: "on"
            then:
              - service: input_text.set_value
                target: { entity_id: input_text.aut_test_observed }
                data: { value: "default_branch" }
```

### 2. Write the Automation Wrapper

```yaml
automation:
  - id: 'my_automation_trigger'
    alias: 'My Automation: Trigger'
    mode: single
    trigger:
      - platform: state
        entity_id: binary_sensor.motion
        to: 'on'
    action:
      - service: script.my_automation_logic
        data:
          sensor_state: "{{ states('binary_sensor.motion') }}"
          threshold: "{{ states('sensor.threshold') | int }}"
```

### 3. Write Tests

```python
# pyscript/test_my_automation.py
@service
def run_my_automation_tests():
    service.call("pyscript", "reset_test_counters")
    test_branch_1()
    test_default()
    task.sleep(1)
    service.call("pyscript", "get_test_summary")

def test_branch_1():
    service.call("pyscript", "run_script_test",
        script_id="my_automation_logic",
        scenario_name="on + threshold 60 → branch 1",
        script_data={"sensor_state": "on", "threshold": 60},
        expected_branch="branch_1_executed",
        timeout_seconds=5.0)
```

### 4. Run and Verify

```bash
homeassist services call pyscript.run_my_automation_tests
homeassist entities get counter.aut_test_pass --compact
homeassist entities get counter.aut_test_fail --compact
```

## Red-Green-Refactor Cycle

1. **RED** — Write test with expected branch → fails (script doesn't exist)
2. **GREEN** — Create script with `choose:` + observations → passes
3. **REFACTOR** — Clean up, keep tests green

## Prerequisites

Test infrastructure package must be deployed:

```yaml
# packages/pyscript_test_infrastructure.yaml
input_boolean:
  pyscript_test_mode: { name: "PyScript Test Mode" }
input_text:
  aut_test_observed: { name: "Automation Test Observation", max: 255 }
counter:
  aut_test_pass: { name: "Tests Passed" }
  aut_test_fail: { name: "Tests Failed" }
  aut_test_total: { name: "Tests Run" }
```

Plus the test runner at `pyscript/automation_test_runner.py`.

## Anti-Patterns

| Don't | Do Instead |
|-------|-----------|
| `automation.trigger` for testing | Call script with explicit inputs |
| Read global state in script | Pass via `fields:` parameters |
| `task.sleep()` for waiting | `task.wait_until()` with state trigger |
| Skip test observation | Every branch emits observation when test mode on |

## Completion Checklist

- [ ] Script has explicit `fields:` for all inputs
- [ ] Every `choose:` branch has test observation
- [ ] Default branch has test observation
- [ ] Thin automation wrapper calls script with explicit data
- [ ] Test file covers all branches
- [ ] All tests pass
