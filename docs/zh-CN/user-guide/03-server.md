# 03 — 服务部署

> **目标**: 把 `mah start` 跑成长寿命服务,gRPC + HTTP 都有,
> 配反代 + HTTPS + 简单 auth。

[English](03-server.md) | [简体中文](03-server.md)

## 前置条件

- 装好 `mah` CLI (见 [01-installation.md](01-installation.md))
- 一台稳定网络的机器 (VM / 容器 / 物理机)
- 反代 (nginx / Caddy / Envoy / Cloudflare Tunnel)
- TLS 证书 (Let's Encrypt 或公司 CA)
- ~20 分钟

## `mah start` 跑什么

一个进程跑 3 个东西:

| 服务 | 默认端口 | 协议 | 作用 |
|---|---|---|---|
| **gRPC** `AgentService` | 50051 | HTTP/2 (gRPC) | 流式 agent run |
| **gRPC** `SessionService` | 50051 | HTTP/2 (gRPC) | session 生命周期 (Create / Get / List / Close) |
| **gRPC** `EventService` (P5+) | 50051 | HTTP/2 (gRPC) | event log 查询 |
| **HTTP** `/health` `/version` `/v1/...` | 50050 | HTTP/1.1 (salvo) | REST + OpenAPI spec |

端口可配。日志去 stderr (可选文件)。

## 步骤

### 第 1 步 — localhost 烟雾测试

```bash
mah start --grpc-port 50051 --http-port 50050 &
# 等一下启动
sleep 2

# 检查 health
curl http://localhost:50050/health
# 期望: 200 OK

# 检查 version
curl http://localhost:50050/version
# 期望: {"version": "0.1.0"}
```

### 第 2 步 — 持久化 session 存储

默认 session 在内存 (重启丢失)。持久化:

```bash
mah start \
  --grpc-port 50051 \
  --http-port 50050 \
  --store-path /var/lib/mah-harness/sessions.db
```

这会建一个 SQLite 数据库存 session 状态。

### 第 3 步 — nginx 反代 + HTTPS

建 `/etc/nginx/sites-available/mah-harness`:

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

启用 + 重新加载:

```bash
sudo ln -s /etc/nginx/sites-available/mah-harness /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

### 第 4 步 — 从其他机器 gRPC 客户端

用 `grpcurl` (调试神器):

```bash
grpcurl -insecure mah.example.com:443 list
# 期望: mah_harness.v1.AgentService, mah_harness.v1.SessionService

grpcurl -insecure -d '{"session_id": "demo", "message": "hello"}' \
  mah.example.com:443 mah_harness.v1.AgentService/Run
```

或用 `mah` 本身 (P11+ RunStream):

```bash
mah run-stream --grpc-url https://mah.example.com:443 "hello"
```

### 第 5 步 — Authentication (推荐)

`mah start` 暂不带 auth (P5+ 计划)。用反代强制:

**nginx basic auth:**

```nginx
location / {
    auth_basic "mah-harness";
    auth_basic_user_file /etc/nginx/.htpasswd;

    proxy_pass http://mah_http;
    # ...
}
```

**Cloudflare Access** (零信任):

1. 把 mah.example.com 加到 Cloudflare
2. Cloudflare Zero Trust → Applications → Add → Self-hosted
3. 策略: 任何已认证用户
4. Cloudflare tunnel gRPC 流量,mah 只看到 Cloudflare IP

### 第 6 步 — systemd 服务

建 `/etc/systemd/system/mah-harness.service`:

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

## 验证

```bash
# Health
curl https://mah.example.com/health
# 期望: 200

# OpenAPI spec 可用
curl https://mah.example.com/openapi.json | jq .info.title
# 期望: "ma-harness.rs"

# gRPC 通过 nginx 工作
grpcurl -insecure mah.example.com:443 list
```

## 资源规划

| 负载 | CPU | 内存 | 磁盘 (每 1M 事件) |
|---|---|---|---|
| 轻 (< 10 req/min) | 1 core | 512 MB | ~200 MB |
| 中 (10-100 req/min) | 2 cores | 2 GB | ~2 GB |
| 重 (100-1000 req/min) | 4 cores | 8 GB | ~20 GB |

Event log 是 append-only SQLite。每月 `sqlite3 events.db VACUUM;`。

## 下一步

- 加 **插件** 提供真实能力 — 见 [04-plugins.md](04-plugins.md)
- **验证** 部署 — 见 [05-conformance.md](05-conformance.md)
- **扩展** 多 server: gRPC 无状态;session 用共享 Postgres (P12+)

## Troubleshooting

### gRPC 过 nginx: "connection reset by peer"

nginx 的 listen 指令需要 `http2` (HTTP/2 cleartext → gRPC):

```nginx
listen 443 ssl http2;   # ← http2 必填
```

### 启动报 "address already in use"

别的进程占着端口。找它:

```bash
# Linux
sudo lsof -i :50051

# Windows
Get-NetTCPConnection -LocalPort 50051
```

### LLM API timeout 时 server 崩

`mah` 还没 retry LLM。临时调大 `--timeout-ms` (P5+):

```bash
mah start --timeout-ms 60000
```

或在前面套 retry proxy (unreliable / maverick)。

### 内存无限增长

`mah start` 把所有 active session 放内存。检查:

```bash
mah sessions list
# Count: 1234 sessions
```

太多就定期重启 (systemd `RestartSec=300`)。
