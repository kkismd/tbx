#!/bin/bash
# preToolUse hook: block any tool invocation that would write to /tmp or /private/tmp.
#
# Checked patterns:
#   - bash tool: command string contains /tmp or /private/tmp
#   - create/edit tools: file path starts with /tmp or /private/tmp

set -euo pipefail

INPUT=$(cat)

TOOL_NAME=$(echo "$INPUT" | jq -r '.toolName')
TOOL_ARGS=$(echo "$INPUT" | jq -r '.toolArgs')

TMP_PATTERN='(/tmp|/private/tmp)'

deny() {
  local reason="$1"
  jq -n --arg r "$reason" \
    '{"permissionDecision":"deny","permissionDecisionReason":$r}'
  exit 0
}

case "$TOOL_NAME" in
  bash)
    COMMAND=$(echo "$TOOL_ARGS" | jq -r '.command // ""')
    if echo "$COMMAND" | grep -qE "$TMP_PATTERN"; then
      deny "/tmp へのアクセスはセキュリティポリシーにより禁止されています。一時ファイルは .tmp/ 配下を使用してください。"
    fi
    ;;
  create|edit)
    FILE_PATH=$(echo "$TOOL_ARGS" | jq -r '.path // ""')
    if echo "$FILE_PATH" | grep -qE "^$TMP_PATTERN"; then
      deny "/tmp へのファイル書き込みはセキュリティポリシーにより禁止されています。一時ファイルは .tmp/ 配下を使用してください。"
    fi
    ;;
esac

# Allow all other tool invocations
exit 0
