# 安装 agent-runtime

简体中文 · [English](INSTALL.md)

如何在本机跑起 `agent-runtime`。`agent-runtime` 是执行机,它向 Shepherd 服务端注册、长轮询认领任务,并调用你的 AI CLI(Claude Code / Codex / OpenCode / CodeBuddy)。服务端和 Web 控制台需另行部署——见[部署与运维](DEPLOYMENT.zh-CN.md),单机 Linux 部署可参考 [GHCR docker-compose](../deploy/docker/docker-compose.ghcr.yml)。

按系统选择安装方式:

| 系统 | 推荐 | 备选 |
|---|---|---|
| macOS | Homebrew | 手动二进制 |
| Windows | Scoop / 一键 PowerShell | 手动二进制 |
| Linux | 手动二进制 (tar.gz) | Docker |

所有二进制制品都附在每个 `v*` 标签的 GitHub Release 中:`agent-runtime-x86_64-unknown-linux-gnu.tar.gz`、`aarch64-unknown-linux-gnu`、`x86_64-apple-darwin`、`aarch64-apple-darwin`、`x86_64-pc-windows-msvc.zip`,以及 `checksums.txt`。

> **制品未签名。** `agent-runtime` 是纯 Rust(rustls-tls,无 OpenSSL),所以交叉编译出的二进制不带代码签名。macOS Gatekeeper 和 Windows SmartScreen 都会先拦截,需要你手动放行——见下文各系统说明。

---

## macOS — Homebrew

Formula 在 `muxso/homebrew-shepherd` 这个 tap 里,Brew 会根据 Release 元数据自动更新。

```bash
brew install muxso/shepherd/agent-runtime
agent-runtime --help
```

日后升级:

```bash
brew upgrade muxso/shepherd/agent-runtime
```

### 首次运行放行 Gatekeeper(仅一次)

因为二进制未签名,macOS 会拒绝打开。清除一次隔离标志即可:

```bash
xattr -d com.apple.quarantine "$(brew --prefix muxso/shepherd/agent-runtime)/bin/agent-runtime"
```

如果你是用 tarball 手动装的(没走 brew),把 `xattr` 指向你放二进制的位置,例如:

```bash
xattr -d com.apple.quarantine /usr/local/bin/agent-runtime
```

---

## Windows — Scoop 或 PowerShell

### 方式 A:Scoop(推荐)

```powershell
scoop bucket add shepherd https://github.com/muxso/scoop-bucket
scoop install shepherd-agent-runtime
agent-runtime --help
```

Scoop 的 manifest 里有 `autoupdate`,会跟随新 Release,所以 `scoop update` 就能拉到新版本。

### 方式 B:一键 PowerShell 安装

下载最新 Windows zip,解压到 `%LOCALAPPDATA%\shepherd`,并加入用户 PATH:

```powershell
irm https://raw.githubusercontent.com/muxso/Shepherd/main/scripts/install.ps1 | iex
```

之后**重开**一个终端(PATH 变动对已经打开的 shell 不生效),然后:

```powershell
agent-runtime --help
```

> 若 SmartScreen 拦截,选 "More info → Run anyway",或手动放行一次:`Unblock-File -Path "$env:LOCALAPPDATA\shepherd\agent-runtime.exe"`。

---

## Linux — 手动二进制

```bash
# 选你的架构
curl -sSL https://github.com/muxso/Shepherd/releases/latest/download/agent-runtime-x86_64-unknown-linux-gnu.tar.gz -o rt.tar.gz
sudo tar -xzf rt.tar.gz -C /usr/local/bin agent-runtime
agent-runtime --help
```

ARM64 用 `agent-runtime-aarch64-unknown-linux-gnu.tar.gz`。Linux 下无需隔离 / SmartScreen 这一步。

---

## 接入服务端

装好后,把执行机注册到你的 Shepherd 服务端。你需要服务端的 base URL 和一个 **agent key**(`AccessKey.SecretKey`,形如 `sak_<16hex>.<32hex>`),在 Web 控制台的 *API Keys* 下创建。

```bash
SHEPHERD_BASE=http://<server>:8088 \
SHEPHERD_CAPS=CLAUDE_CODE \
SHEPHERD_AGENT_KEY=sak_f7a83a85536c4391.3b9f... \
agent-runtime
```

| 变量 | 默认值 | 含义 |
|---|---|---|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | 执行机长轮询的服务端地址 |
| `SHEPHERD_CAPS` | `CLAUDE_CODE` | 逗号分隔的能力(认领哪类任务) |
| `SHEPHERD_AGENT_KEY` | — | 来自 *API Keys* 的 `AccessKey.SecretKey` |
| `AGENT_CONCURRENCY` | `1` | 并发任务上限 |
| `CLAUDE_BIN` / `CODEX_CMD` / `OPENCODE_CMD` | `claude` / `codex exec` / `opencode run` | 各 CLI 调用 |
| `AGENT_MOCK` | — | 设置即用 mock 后端(免真实 CLI) |

针对 **CodeBuddy**,用同样方式注册并让它认领任务:

```bash
SHEPHERD_BASE=http://<server>:8088 \
SHEPHERD_CAPS=CODEBUDDY \
SHEPHERD_AGENT_KEY=sak_xxxx.yyyy \
agent-runtime
```

更多执行机后端与接线细节见[运行 AI 执行者](EXECUTORS.zh-CN.md)。

---

## 校验下载

每个 Release 都附带 `checksums.txt`(SHA-256)。信任二进制前先校验:

```bash
curl -sSL https://github.com/muxso/Shepherd/releases/latest/download/checksums.txt -o checksums.txt
sha256sum -c checksums.txt
```
