# Shepherd — Docker (single-host dev/demo stack)

A self-contained local stack: PostgreSQL 16, Redis 7, the Rust `server` and
`agent-runtime`, and the nginx-served web SPA. This is for **development and
demos on one host** — not production (use `deploy/helm` / `deploy/terraform`
for that).

## Run

```bash
docker compose -f deploy/docker/docker-compose.yml up --build
```

The server runs DB migrations on startup; first boot may take a moment while
images build and Postgres becomes healthy.

## Defaults

| Service | URL |
|---------|-----|
| Web UI  | http://localhost:8080 |
| API     | http://localhost:8088 |

- **Web login:** `admin` / `change-me-please`
  (set via `SHEPHERD_ADMIN_PASSWORD` in `docker-compose.yml` — change it).
- **agent-runtime auth:** API key only (`SHEPHERD_AGENT_KEY` in `.env`). On a
  fresh stack, boot server + web first, issue a key (web: 个人中心 → API KEY,
  or `POST /system/apikey` with `DELIVERY:UPDATE` + `REQUIREMENT:UPDATE`),
  put it in `deploy/docker/.env`, then `docker compose up -d agent-runtime`.
- `agent-runtime` runs with `AGENT_MOCK=1` (no real Claude/Codex CLIs). For
  real backends, add the CLIs to the agent-runtime image or bind-mount them and
  drop `AGENT_MOCK`.
- Health: `GET /healthz` (liveness), `GET /readyz` (ready once Postgres is up).

## Images

The three Dockerfiles build from the **repo root** context:

- `Dockerfile.server` — Rust bin `server`, EXPOSE 8088.
- `Dockerfile.agent-runtime` — Rust bin `agent-runtime`, no ports, includes `git`.
- `Dockerfile.web` — Vite build → nginx (`nginx.conf` is canonical), EXPOSE 80.

Tear down (and wipe the Postgres volume):

```bash
docker compose -f deploy/docker/docker-compose.yml down -v
```
