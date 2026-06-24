#!/usr/bin/env bash
# Codex 异步桥接:设 CLI_CMD 后转通用桥接。CODEX_CMD 可覆盖实际命令(按你的 codex 版本调整)。
set -uo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
CLI_CMD="${CODEX_CMD:-codex exec}" exec bash "$DIR/cli-agent-async.sh"
