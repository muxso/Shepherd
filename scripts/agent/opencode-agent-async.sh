#!/usr/bin/env bash
# OpenCode 异步桥接:设 CLI_CMD 后转通用桥接。OPENCODE_CMD 可覆盖实际命令(按你的 opencode 版本调整)。
set -uo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
CLI_CMD="${OPENCODE_CMD:-opencode run}" exec bash "$DIR/cli-agent-async.sh"
