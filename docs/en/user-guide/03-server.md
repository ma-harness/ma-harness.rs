# 03 — Server Deployment

> **Goal**: run `mah start` as a long-lived server with gRPC + HTTP,
> behind a reverse proxy, with HTTPS and basic auth.

[English](03-server.md) | [简体中文](../../zh-CN/user-guide/03-server.md)

## Prerequisites

- `mah` CLI installed (see [01-installation.md](01-installation.md))
- A machine with stable network (VM, container, bare metal)
- Reverse proxy of choice: nginx / Caddy / Envoy / Cloudflare Tunnel
- TLS cert (Let's Encrypt, or your corporate CA)
- ~20 minutes

## What `mah start` does

`mah start` runs three things in one process:

| Service | Default port | Protocol | Purpose |
|---|---|---|---|
| **gRPC** `AgentService` | 50051 | HTTP/2 (gRPC) | streaming agent runs |
| **gRPC** `SessionService` | 50051 | HTTP/2 (gRPC) | session lifecycle (Create / Get / List / Close) |
| **gRPC** `EventService` (P5+) | 50051 | HTTP/2 (gRPC) | event log query |
| **HTTP** `/health` `/version` `/v1/...` | 50050 | HTTP/1.1 (salvo) | REST + OpenAPI spec |

Ports are configurable. Logs go to stderr (and optionally a file).

## Step-by-step

### Step 1 — Smoke test on localhost

```bash
mah start --grpc-port 50051 --http-port 50050 &
# Wait a moment for startup
sleep 2

# Check health
curl http://localhost:50050/health
# Expected: 200 OK

# Check version
curl http://localhost:50050/version
# Expected: {"version": "0.1.0"}
```

### Step 2 — Run with persistent session storage

By default, sessions are in-memory (lost on restart). To persist:

```bash
mah start \
  --grpc-port 50051 \
  --http-port 50050 \
  --store-path /var/lib/mah-harness/sessions.db
```

This creates a SQLite database for session state.

### Step 3 — Reverse proxy with nginx + HTTPS

Create `/etc/nginx/sites-available/mah-harness`:

```nginx
upstream mah_grpc {
    server 127.0.0.1:50051;
    keepalive 32;
}

upstream mah_http {
    server 127.0.0.1:50050;
    keepalive 32;
}

server {
    listen 443 ssl http2;
    server_name mah.example.com;

    ssl_certificate     /etc/letsencrypt/live/mah.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/mah.example.com/privkey.pem;

    # HTTP REST + OpenAPI
    location / {
        proxy_pass http://mah_http;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # gRPC (HTTP/2 required)
    location /mah_harness.v1.AgentService/ {
        grpc_pass grpc://mah_grpc;
    }
    location /mah_harness.v1.SessionService/ {
        grpc_pass grpc://mah_grpc;
    }
}
```

Enable and reload:

```bash
sudo ln -s /etc/nginx/sites-available/mah-harness /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

### Step 4 — gRPC client from another machine

Using `grpcurl` (great for testing):

```bash
grpcurl -insecure mah.example.com:443 list
# Expected: mah_harness.v1.AgentService, mah_harness.v1.SessionService

grpcurl -insecure -d '{"session_id": "demo", "message": "hello"}' \
  mah.example.com:443 mah_harness.v1.AgentService/Run
```

Or with `mah` itself (P11+ RunStream):

```bash
mah run-stream --grpc-url https://mah.example.com:443 "hello"
```

### Step 5 — Authentication (recommended)

`mah start` does not include auth out of the box (P5+ planned).
Use your reverse proxy to enforce it:

**nginx basic auth:**

```nginx
location / {
    auth_basic "mah-harness";
    auth_basic_user_file /etc/nginx/.htpasswd;

    proxy_pass http://mah_http;
    # ...
}
```

**Cloudflare Access** (zero-trust):

1. Add mah.example.com to Cloudflare
2. Cloudflare Zero Trust → Applications → Add → Self-hosted
3. Set policy: any authenticated user
4. Cloudflare tunnels gRPC traffic, mah sees only Cloudflare IPs

### Step 6 — Run as a systemd service

Create `/etc/systemd/system/mah-harness.service`:

```ini
[Unit]
Description=ma-harness.rs server
After=network.target

[Service]
Type=simple
User=mah
Group=mah
WorkingDirectory=/var/lib/mah-harness
ExecStart=/usr/local/bin/mah start \
  --grpc-port 50051 \
  --http-port 50050 \
  --store-path /var/lib/mah-harness/sessions.db
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now mah-harness
sudo systemctl status mah-harness
```

## Verify

```bash
# Health
curl https://mah.example.com/health
# Expected: 200

# OpenAPI spec available
curl https://mah.example.com/openapi.json | jq .info.title
# Expected: "ma-harness.rs"

# gRPC works through nginx
grpcurl -insecure mah.example.com:443 list
```

## Resource planning

| Workload | CPU | RAM | Disk (per 1M events) |
|---|---|---|---|
| Light (< 10 req/min) | 1 core | 512 MB | ~200 MB |
| Medium (10-100 req/min) | 2 cores | 2 GB | ~2 GB |
| Heavy (100-1000 req/min) | 4 cores | 8 GB | ~20 GB |

The event log is append-only SQLite. Vacuum monthly with `sqlite3 events.db VACUUM;`.

## What's next

- Add **plugins** for real capabilities — see [04-plugins.md](04-plugins.md)
- **Validate** your deployment with conformance tests — see [05-conformance.md](05-conformance.md)
- **Scale** beyond a single server: gRPC is stateless; sessions can be
  in a shared Postgres (P12+)

## Troubleshooting

### gRPC over nginx: "connection reset by peer"

nginx needs `http2` on the listen directive (for HTTP/2 cleartext → gRPC):

```nginx
listen 443 ssl http2;   # ← http2 is required
```

### "address already in use" on startup

Another process is using the port. Find it:

```bash
# Linux
sudo lsof -i :50051

# Windows
Get-NetTCPConnection -LocalPort 50051
```

### Server crashes on LLM API timeout

`mah` does not yet retry on LLM API failures. As a workaround, set
a higher `--timeout-ms` (P5+):

```bash
mah start --timeout-ms 60000
```

Or run behind a retry proxy like `unreliable` or `maverick`.

### Memory grows unbounded

The event log is in SQLite (bounded by disk). In-memory caches for
session state grow with active sessions. Restart periodically:

```bash
# Watchdog pattern in systemd
Restart=on-failure
RestartSec=300
```
