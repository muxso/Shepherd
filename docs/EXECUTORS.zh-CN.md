# Shepherd — AI 执行者运行指南

介绍如何在 `agent-runtime` 下运行各家 AI 执行者(Claude Code / Codex / OpenCode /
CodeBuddy)。机群架构与服务端配置见 [USAGE.zh-CN.md §7](./USAGE.zh-CN.md),
镜像构建与部署见 [DEPLOYMENT.zh-CN.md](./DEPLOYMENT.zh-CN.md)。

- [派发如何到达 CLI](#派发如何到达-cli)
- [runtime 通用配置](#runtime-通用配置)
- [Claude Code](#claude-code)
- [Codex](#codex)
- [OpenCode](#opencode)
- [CodeBuddy](#codebuddy)
- [一个 runtime 承接多种执行者](#一个-runtime-承接多种执行者)
- [mock 执行者(无需 CLI)](#mock-执行者无需-cli)
- [Windows](#windows)
- [Docker 与宿主机 CLI](#docker-与宿主机-cli)
- [共用检出、不同分支](#共用检出不同分支)
- [指定执行者派发](#指定执行者派发)
- [排障](#排障)

---

## 派发如何到达 CLI

每个交付尝试都带 `executor` 类型(`CLAUDE_CODE` / `CODEX` / `OPENCODE` /
`CODEBUDDY`)。服务端按类型入队;`agent-runtime` 以 `SHEPHERD_CAPS` 长轮询,
只认领自己声明过的类型。认领后 runtime 在独立 git worktree 中拉起对应 CLI,
把产生的改动快照成 commit 并回报。

能力隔离是严格的:`SHEPHERD_CAPS=CODEBUDDY` 的 runtime 永远拿不到
`CLAUDE_CODE` 的任务,反之亦然。各类型积压可看 `GET /agent/work/stats`。

## runtime 通用配置

运行 `agent-runtime` 的机器需要 `git` 和对应 CLI 在 `PATH` 上,且 CLI 已完成
登录(runtime 不管理 CLI 的认证)。

```bash
SHEPHERD_BASE=http://<server>:8088 \
SHEPHERD_AGENT_KEY=sak_… \
SHEPHERD_CAPS=<KIND[,KIND…]> \
RUNTIME_NAME=$(hostname) \
AGENT_WORKDIR=/path/to/target/repo \
./agent-runtime
```

### 认证:API key(必填)

runtime 只接受静态 API key 认证,没有账号/口令路径。每台 runtime 发一把
自己的 key:key 永不过期,吊销只影响这一台 runtime,不牵连其他机器。
key 在 Web 控制台(个人中心 → API KEY)或 `POST /system/apikey` 签发:

```bash
# 1. 管理员登录(拿一次管理员 token)
TOKEN=$(curl -s -X POST http://<server>:8088/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"<admin-password>"}' | jq -r .token)

# 2. 按最小权限集创建 key
curl -s -X POST http://<server>:8088/system/apikey \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"name":"runtime-buildbox-1","permissions":["DELIVERY:UPDATE","REQUIREMENT:UPDATE"]}'
# → {"key":"sak_<16hex>.<32hex>", …}  明文只出现这一次,当场保存
```

runtime key 的最小权限集是 `DELIVERY:UPDATE` + `REQUIREMENT:UPDATE`:
runtime 调用的机群接口(注册、心跳、认领、交付 events/complete/fail)全部
校验 `DELIVERY:UPDATE`,设计稿回填(`POST /proposal/{id}/design`)校验
`REQUIREMENT:UPDATE`。不需要 `READ`,也不需要 `EXECUTE`。

把 key 设为 `SHEPHERD_AGENT_KEY` 即可。runtime 直接把它当静态 bearer 用;
收到 `401` 表示 key 已被吊销(重新签发并更新环境变量)。未设 key 时
runtime 启动即报错。

| 环境变量 | 默认 | 含义 |
|---|---|---|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | 服务端地址(仅出站,不需要入站端口) |
| `SHEPHERD_AGENT_KEY` | **必填** | 静态 API key(`sak_…`),唯一凭证;缺失则启动失败 |
| `SHEPHERD_CAPS` | `CLAUDE_CODE` | 本 runtime 认领的执行者类型,逗号分隔 |
| `RUNTIME_NAME` | `agent-runtime` | 机群注册表中的显示名 |
| `AGENT_WORKDIR` | `.` | 任务操作的 git 仓库 |
| `AGENT_BASE_REF` | *(仓库 HEAD)* | 任务基点 ref(如 `origin/main`) |
| `AGENT_CONCURRENCY` | `1` | 最大并发任务数 |
| `AGENT_TASK_TIMEOUT_SECS` | `1800` | 单任务 CLI 超时 |

## Claude Code

唯一的流式后端:跑 `claude -p --output-format stream-json`,把工具调用事件
实时解析进交付决策日志,并带 `--permission-mode acceptEdits`。

```bash
# CLI 在且已登录?
claude --version

SHEPHERD_CAPS=CLAUDE_CODE AGENT_WORKDIR=/repo … ./agent-runtime
```

二进制不在 `PATH` 时用 `CLAUDE_BIN=/opt/claude/bin/claude` 指定。

## Codex

通用(非流式)后端:每任务一次 CLI 调用,stdout 作为结果摘要。

```bash
codex --version

SHEPHERD_CAPS=CODEX AGENT_WORKDIR=/repo … ./agent-runtime
```

默认调用 `codex exec "<prompt>"`;整条命令可用 `CODEX_CMD` 覆盖
(如 `CODEX_CMD="codex exec --full-auto"`)。

## OpenCode

通用后端,形态同 Codex。

```bash
opencode --version

SHEPHERD_CAPS=OPENCODE AGENT_WORKDIR=/repo … ./agent-runtime
```

默认调用 `opencode run "<prompt>"`;用 `OPENCODE_CMD` 覆盖。

## CodeBuddy

通用后端。默认调用
`codebuddy -p --permission-mode acceptEdits "<prompt>"` —— 权限参数是关键:
纯 `-p` 打印模式下 CodeBuddy 会拒绝改文件(Write/Bash 工具等待一个 headless
下永远不会来的审批),导致交付"无代码变动"。

```bash
codebuddy --version

SHEPHERD_CAPS=CODEBUDDY AGENT_WORKDIR=/repo … ./agent-runtime
```

用 `CODEBUDDY_CMD` 覆盖,例如任务需要跑 shell 时放宽权限:
`CODEBUDDY_CMD="codebuddy -p --permission-mode bypassPermissions"`。

## 一个 runtime 承接多种执行者

单个 runtime 可以声明多种类型——全部列出,并保证对应 CLI 都装好且已登录:

```bash
SHEPHERD_CAPS=CLAUDE_CODE,CODEBUDDY … ./agent-runtime
```

每台机器(一套 CLI 登录态)跑一个 runtime;横向扩展就是对同一服务端多起几个
runtime(能力任意组合)。服务端设了 `SHEPHERD_FLEET_REDIS` 时,跨机 runtime
共享同一个队列。

## mock 执行者(无需 CLI)

`AGENT_MOCK=1` 让 runtime 认领其声明的任意类型并返回固定输出,不拉起真实
CLI——适合在装 CLI 之前先冒烟验证派发链路。

```bash
AGENT_MOCK=1 SHEPHERD_CAPS=CLAUDE_CODE,CODEX,OPENCODE,CODEBUDDY … ./agent-runtime
```

## Windows

runtime 可在 Windows 原生运行(MSVC 工具链:`cargo build --release -p agent-runtime`)。
两个平台差异:

- npm 装的 CLI(`claude` / `codebuddy` / `opencode`)在 Windows 上是 `<name>.cmd`
  垫片。裸程序名在 `PATH` 里找不到 `<name>.exe` 时,runtime 会自动解析到垫片
  (`codex.exe` 这类原生二进制优先);`CLAUDE_BIN` / `*_CMD` 里写的显式路径原样使用。
  `CLAUDE_BIN=D:\nvm4w\nodejs\claude.cmd` 没问题:Claude 后端的 prompt 走 stdin,
  经 cmd.exe 一跳无害。
- **通用后端(codebuddy/codex/opencode)不能走 `.cmd` 垫片**:它们把 prompt 作为
  命令行参数传,prompt 必含换行,而 Rust 拒绝给批处理文件传含换行的参数
  (cmd.exe 无法安全携带)。绕开 cmd.exe,直接用 node 调包入口——先
  `type codebuddy.cmd` 看垫片真正调用什么,然后照抄,如
  `CODEBUDDY_CMD=node D:\nvm4w\nodejs\node_modules\@tencent-ai\codebuddy-code\cli.js -p --permission-mode acceptEdits`。
- 任务超时只会终止 CLI 直接进程——Windows 没有进程组语义,CLI 再往下派生的
  子进程可能残留。

## Docker 与宿主机 CLI

Linux 容器**执行不了宿主机的 Windows(或 macOS)CLI 二进制**——把
`claude.cmd` / `codebuddy.exe` 挂载进容器是行不通的。可行的做法反过来拆:

- 镜像里装 **Linux 版** CLI;
- 只把宿主机的 CLI **凭据/配置目录**挂进去,登录态在镜像重建后仍然有效。

```dockerfile
FROM shepherd-agent-runtime
USER root
RUN apt-get update && apt-get install -y --no-install-recommends nodejs npm \
 && npm install -g @anthropic-ai/claude-code @tencent-ai/codebuddy-code \
 && rm -rf /var/lib/apt/lists/*
USER shepherd
```

```yaml
  agent-runtime:
    volumes:
      - ~/.claude:/home/shepherd/.claude          # CLI 登录态
      - ~/.codebuddy:/home/shepherd/.codebuddy
      - /path/to/repo:/work                       # AGENT_WORKDIR=/work
```

Windows 宿主机同理——Docker Desktop 能把 `C:\Users\me\.claude` 挂进 Linux
容器;必须是 Linux 构建的是 CLI *二进制*,配置目录没有这个限制。

## 共用检出、不同分支

每个任务都在**分离(detached)git worktree** 里运行:基仓库的已检出分支不会被
切走,工作区也不会被碰——runtime(原生或容器)可以安全地和开发者共用同一份
clone,即便各自在不同分支上。两点须知:

- 默认任务基点是共用 clone 当前检出的 `HEAD`——会跟着开发者切分支而变。设
  `AGENT_BASE_REF=origin/main`(或分支/tag/SHA)可把基点钉死。remote-tracking
  ref 解析到上次 fetch 的位置,要前移先 fetch。
- worktree 建在 runtime 的临时目录下。runtime 在容器里时,记录的是容器内路径,
  宿主机 `git worktree list` 可能看到失效条目——无害,runtime 每次任务前会 prune。

## 指定执行者派发

执行者在派发时按任务指定,不是服务端全局配置:

```bash
# HTTP
curl -X POST $BASE/delivery -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"decompositionId":"d1","taskId":"t1","title":"…","executor":"CODEBUDDY"}'

# CLI
shepherd dispatch --decomp d1 --task t1 --executor CODEBUDDY
# 或先绑定默认:shepherd agent connect --kind codebuddy
```

MCP 工具 `shepherd_dispatch_delivery` 同样接受这四种类型。

### 定向到具体某台 runtime

除了按类型,还可以把任务钉给某个注册上来的 runtime(`targetRuntime` = 注册名,
即该 runtime 的 `RUNTIME_NAME`;名字跨重连稳定,runtime id 每次注册会变):

```bash
curl -X POST $BASE/delivery -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"decompositionId":"d1","taskId":"t1","title":"…","executor":"CLAUDE_CODE","targetRuntime":"box-1"}'
```

定向任务只会被同名 runtime 认领(Redis 下走 `fleet:rt:<name>` 专属流);目标离线时任务留队等它回来,
不会被其它 runtime 抢走。Web 端在拆分图的派发菜单里选具体执行者即可,离线的会置灰。
同名多实例会共享这条定向流——需要一对一就给每台起唯一的名字。

## 排障

| 现象 | 排查 |
|---|---|
| 任务一直排队(`ready` 增长) | 没有在线 runtime 声明该类型——核对 `SHEPHERD_CAPS` 与任务 `executor`,再看 `GET /agent/runtime` 的在线状态 |
| 交付显示"无代码变动" | CLI 跑了但拒绝改文件(权限模式),或提示词本身没要求改文件——看交付摘要,里面是 CLI 的原始输出 |
| 事件里报 spawn 错误 | CLI 不在 runtime 进程的 `PATH` 上——设 `CLAUDE_BIN` / `CODEX_CMD` / `OPENCODE_CMD` / `CODEBUDDY_CMD` |
| 任务超时失败 | 调大 `AGENT_TASK_TIMEOUT_SECS`;通用后端到点会被整进程组杀掉 |
