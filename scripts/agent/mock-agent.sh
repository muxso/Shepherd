#!/usr/bin/env bash
# 确定性「演示/验证」执行者 —— 不调用真实 AI、不改任何文件。
#
# 用途:证明「派发 → AgentExecutor」这条链路是真的(非 Echo 桩),
#       并给前端/演示一个稳定、可复现的交付结果。
#
# 协议(见 crates/delivery/src/adapters/local.rs):
#   - stdin 收到一段 WorkSpec prompt(任务标题/描述/验收标准);
#   - stdout 按行输出:
#       {"event":"<KIND>","message":"...","detail":"..."}  → 审计事件(实时回流)
#       {"reference":"...","summary":"..."}                → 交付物(Delivered)
#   - 退出码 0 = 成功;非 0 = 失败(executor 报错)。
#   合法 KIND: DECISION / FILE_CHANGE / TEST_RESULT / TOOL_CALL / VERDICT / LOG
set -euo pipefail

prompt="$(cat)" # 吞掉 stdin(WorkSpec),否则子进程可能因管道未读而异常
title="$(printf '%s\n' "$prompt" | sed -n 's/^# Task: //p' | head -1)"
[ -n "${title:-}" ] || title="task"

# 审计轨迹:决策 → 文件变更 → 测试结论(全为 mock,仅演示链路)
printf '{"event":"DECISION","message":"分析任务并制定方案: %s","detail":"mock executor — 演示链路,无真实改动"}\n' "$title"
printf '{"event":"FILE_CHANGE","message":"(mock) 生成示例变更"}\n'
printf '{"event":"TEST_RESULT","message":"(mock) 验收检查通过"}\n'
printf '{"event":"VERDICT","message":"(mock) 任务完成"}\n'

# 交付物:reference 以 mock:// 前缀标明非真实(也明确区别于 Echo 桩的 stub://)
printf '{"reference":"mock://%s","summary":"[mock] 已完成: %s"}\n' "$$" "$title"
