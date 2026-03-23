---
description: Run pyscript automation tests and report results
---

# HA Automation Test Runner

Run pyscript-based automation tests against the live Home Assistant instance:

1. **Reset counters** — `homeassist services call pyscript.reset_test_counters`
2. **Run test suite** — `homeassist services call pyscript.<test_service_name>`
   - If no specific test given, ask the user which test suite to run
   - List available test suites: `homeassist entities search "pyscript.run_.*_tests" --compact`
3. **Wait for completion** — pause 3-5 seconds for tests to execute
4. **Check results**:
   ```
   homeassist entities get counter.aut_test_pass --compact
   homeassist entities get counter.aut_test_fail --compact
   homeassist entities get counter.aut_test_total --compact
   ```
5. **Check observation** — `homeassist entities get input_text.aut_test_observed --compact`
6. **Report** — summarize pass/fail counts

If any tests fail, check the Home Assistant logs:
```
homeassist logs errors --tail 30 --pattern "pyscript"
```
