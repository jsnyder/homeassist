#!/usr/bin/env bash
# Pre-deploy hook: when a bash command includes "config reload" or "ha restart",
# remind the agent to validate config first.
#
# This hook runs on PreToolUse for Bash commands. It inspects the tool input
# and prints advisory context if a deployment action is detected.

TOOL_INPUT="${1:-}"

# Check if the command looks like a deploy/reload action
if echo "$TOOL_INPUT" | grep -qiE '(config reload|ha.restart|homeassist.*reload)'; then
    # Check if config was already validated in this session
    if [ -z "${HA_CONFIG_CHECKED:-}" ]; then
        echo "ADVISORY: Detected config reload/restart. Run 'homeassist config check' first to validate."
    fi
fi
