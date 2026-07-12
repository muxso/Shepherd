# Security Policy

> 简体中文版见 [SECURITY.zh-CN.md](SECURITY.zh-CN.md).

## Reporting a vulnerability

Please do not disclose vulnerabilities in public issues. Use GitHub's private vulnerability reporting (Security → Advisories → New), or email the address listed on the repository owner's profile. We acknowledge reports within 72 hours and coordinate disclosure with you before a fix ships.

## Supported versions

Only the latest `main` receives security fixes while the project is pre-1.0.

## Threat model notes

Before deploying:

- **A compromised server means RCE on every agent-runtime box.** Runtimes pull task prompts from the server and feed them to local AI CLIs. Treat the server as the fleet's root of trust and minimize its exposure.
- **Change the default admin password.** The server warns at startup when `SHEPHERD_ADMIN_PASSWORD` is a weak default. The password is only used for the admin account bootstrap and web login.
- **Runtime credentials: API key only, one per runtime.** Issue each runtime its own key via `POST /system/apikey` with the minimal permission set (`DELIVERY:UPDATE` + `REQUIREMENT:UPDATE`) and set it as `SHEPHERD_AGENT_KEY` (the runtime refuses to start without one); a compromised box is contained by revoking that one key.
- **Terminate TLS at a reverse proxy.** The server listens on plain HTTP; never expose it directly on the public internet.
- **CORS**: put only trusted origins in `SHEPHERD_CORS_ORIGINS`.
- **Rate limiting**: on by default (200 rps per client); tune via `SHEPHERD_RATE_LIMIT_RPS`, set 0 to disable.
