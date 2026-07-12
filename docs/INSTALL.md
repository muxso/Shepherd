# Install

How to get `agent-runtime` running on your machine. `agent-runtime` is the executor that registers to a Shepherd server, long-polls for work, and shells out to your AI CLI (Claude Code / Codex / OpenCode / CodeBuddy). The server and web console are deployed separately — see [Deployment & ops](DEPLOYMENT.md) and the [GHCR docker-compose](../deploy/docker/docker-compose.ghcr.yml) for a single-host Linux deploy.

Pick the install method for your OS:

| OS | Recommended | Alternative |
|---|---|---|
| macOS | Homebrew | manual binary |
| Windows | Scoop / PowerShell one-click | manual binary |
| Linux | manual binary (tar.gz) | Docker |

All binary artifacts are attached to the GitHub Release for each `v*` tag: `agent-runtime-x86_64-unknown-linux-gnu.tar.gz`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc.zip`, plus `checksums.txt`.

> **Builds are unsigned.** `agent-runtime` is plain Rust (rustls-tls, no OpenSSL), so the cross-compiled binaries carry no code signature. macOS Gatekeeper and Windows SmartScreen will both flag them until you bypass — see the per-OS notes below.

---

## macOS — Homebrew

The formula lives in the `muxso/homebrew-shepherd` tap. Brew auto-updates it from the release metadata.

```bash
brew install muxso/shepherd/agent-runtime
agent-runtime --help
```

To upgrade later:

```bash
brew upgrade muxso/shepherd/agent-runtime
```

### Gatekeeper bypass (first run only)

Because the binary is unsigned, macOS will refuse to open it. Clear the quarantine flag once:

```bash
xattr -d com.apple.quarantine "$(brew --prefix muxso/shepherd/agent-runtime)/bin/agent-runtime"
```

If you installed the tarball manually instead of via brew, point `xattr` at wherever you put the binary, e.g.:

```bash
xattr -d com.apple.quarantine /usr/local/bin/agent-runtime
```

---

## Windows — Scoop or PowerShell

### Option A: Scoop (recommended)

```powershell
scoop bucket add shepherd https://github.com/muxso/scoop-bucket
scoop install shepherd-agent-runtime
agent-runtime --help
```

Scoop keeps the manifest's `autoupdate` block in sync with new releases, so `scoop update` pulls new versions.

### Option B: one-click PowerShell installer

Downloads the latest Windows zip, extracts to `%LOCALAPPDATA%\shepherd`, and adds it to your user PATH:

```powershell
irm https://raw.githubusercontent.com/muxso/Shepherd/main/scripts/install.ps1 | iex
```

Open a **new** terminal afterwards (PATH changes don't apply to already-open shells), then:

```powershell
agent-runtime --help
```

> If SmartScreen blocks it, choose "More info → Run anyway", or unblock the file once: `Unblock-File -Path "$env:LOCALAPPDATA\shepherd\agent-runtime.exe"`.

---

## Linux — manual binary

```bash
# pick your arch
curl -sSL https://github.com/muxso/Shepherd/releases/latest/download/agent-runtime-x86_64-unknown-linux-gnu.tar.gz -o rt.tar.gz
sudo tar -xzf rt.tar.gz -C /usr/local/bin agent-runtime
agent-runtime --help
```

On ARM64 use `agent-runtime-aarch64-unknown-linux-gnu.tar.gz`. No quarantine / SmartScreen step is needed on Linux.

---

## Point it at a server

Once installed, register the executor to your Shepherd server. You need the server's base URL and an **agent key** (`AccessKey.SecretKey`, shape `sak_<16hex>.<32hex>`), created in the web console under *API Keys*.

```bash
SHEPHERD_BASE=http://<server>:8088 \
SHEPHERD_CAPS=CLAUDE_CODE \
SHEPHERD_AGENT_KEY=sak_f7a83a85536c4391.3b9f... \
agent-runtime
```

| Variable | Default | Meaning |
|---|---|---|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | server address the executor long-polls |
| `SHEPHERD_CAPS` | `CLAUDE_CODE` | comma-separated capabilities (which tasks it claims) |
| `SHEPHERD_AGENT_KEY` | — | `AccessKey.SecretKey` from *API Keys* |
| `AGENT_CONCURRENCY` | `1` | max concurrent tasks |
| `CLAUDE_BIN` / `CODEX_CMD` / `OPENCODE_CMD` | `claude` / `codex exec` / `opencode run` | each CLI invocation |
| `AGENT_MOCK` | — | set to use the mock backend (no real CLI) |

For **CodeBuddy** specifically, register it the same way and let it claim work:

```bash
SHEPHERD_BASE=http://<server>:8088 \
SHEPHERD_CAPS=CODEBUDDY \
SHEPHERD_AGENT_KEY=sak_xxxx.yyyy \
agent-runtime
```

More executor backends and wiring details are in [Running AI executors](EXECUTORS.md).

---

## Verifying the download

Every release ships `checksums.txt` (SHA-256). Verify before trusting a binary:

```bash
curl -sSL https://github.com/muxso/Shepherd/releases/latest/download/checksums.txt -o checksums.txt
sha256sum -c checksums.txt
```
