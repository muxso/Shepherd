#!/usr/bin/env bash
# MySQL → PostgreSQL 数据迁移驱动脚本。
#
# 用法:
#   export MYSQL_URL="mysql://user:pass@mysql-host:3306/metersphere"
#   export PG_URL="postgres://msuser:mspass@pg-host:5432/mstest"
#   ./migration/run.sh            # 正式迁移
#   DRY_RUN=1 ./migration/run.sh  # 仅打印将执行的命令,不动数据
#
# 顺序:① 确保 PG schema 已由 ms-migrate 建好 → ② pgloader 迁数据 → ③ 行数核对。
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
LOAD_FILE="$HERE/mysql-to-pg.load"

: "${MYSQL_URL:?需要设置 MYSQL_URL}"
: "${PG_URL:?需要设置 PG_URL(也用于 ms-migrate)}"

command -v pgloader >/dev/null || { echo "未找到 pgloader,请先安装(brew install pgloader)"; exit 1; }

echo "① 应用 PG 迁移(建表,幂等)..."
if [[ "${DRY_RUN:-0}" == "1" ]]; then
  echo "   [dry-run] DATABASE_URL=$PG_URL cargo run -p ms-server -- --migrate-only"
else
  DATABASE_URL="$PG_URL" cargo run -q -p ms-server -- --migrate-only
fi

echo "② pgloader 迁移数据..."
# 把占位符替换为真实连接串后交给 pgloader
TMP="$(mktemp)"
sed -e "s#mysql://MS_USER:MS_PASS@MYSQL_HOST:3306/metersphere#$MYSQL_URL#" \
    -e "s#postgresql://msuser:mspass@PG_HOST:5432/mstest#${PG_URL/postgres:/postgresql:}#" \
    "$LOAD_FILE" > "$TMP"
if [[ "${DRY_RUN:-0}" == "1" ]]; then
  echo "   [dry-run] pgloader $TMP"; cat "$TMP"
else
  pgloader "$TMP"
fi
rm -f "$TMP"

echo "③ 行数核对(人工对照 MySQL 与 PG 的关键表计数):"
echo "   PG:  psql \"$PG_URL\" -c \"SELECT 'ms_user',count(*) FROM ms_user UNION ALL SELECT 'ms_project',count(*) FROM ms_project;\""
echo "完成。建议再跑一段影子流量对比 Rust 服务与 Java 服务的响应一致性,再切换。"
