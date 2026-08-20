# 03 — 服务部署

> **目标**: 把 `mah start` 跑成长寿命服务,gRPC + HTTP 都有,
> 配反代 + HTTPS + 简单 auth。
> 先选你的操作系统,跳到对应的部署章节。

[English](../../en/user-guide/03-server.md) | [简体中文](03-server.md)

## 选你的系统

| 系统 | 服务管理器 | 反向代理 | 跳到 |
|---|---|---|---|
| **Linux** (Debian / Ubuntu / Fedora / Arch) | systemd | nginx | [Linux 部署](#linux) |
| **macOS** (Sonoma / Sequoia) | launchd | nginx 或 Caddy (Homebrew) | [macOS 部署](#macos) |
| **Windows** (10 / 11, Server 2019/2022) | Windows Service (NSSM / sc.exe) | IIS ARR 或 nginx | [Windows 部署](#windows) |

如果只想在 **localhost 试一下**, 直接跳
[快速烟雾测试](#-smoke-test-) — 三个系统都能跑。

## 通用前置条件 (三个系统都要)

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

## 快速烟雾测试 (Smoke test, 三个系统通用)

在配服务管理器和反代之前, 先验证 `mah start` 在本机能跑:

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

前台进程按 **Ctrl-C** 停。`&` 跑后台的话, `kill %1` (Linux/macOS) 或
`Stop-Process -Id $PID` (Windows PowerShell) 停。

## 持久化 session 存储

默认 session 在内存 (重启丢失)。持久化, 把 `--store-path` 指向可写位置:

| 系统 | 推荐路径 |
|---|---|
| **Linux** | `/var/lib/mah-harness/sessions.db` |
| **macOS** | `/usr/local/var/mah-harness/sessions.db` (Intel) 或 `/opt/homebrew/var/mah-harness/sessions.db` (Apple Silicon) |
| **Windows** | `C:\ProgramData\mah-harness\sessions.db` |

先建目录 (各 OS 章节有具体命令), 然后:

```bash
# Linux / macOS
mah start \
  --grpc-port 50051 \
  --http-port 50050 \
  --store-path /var/lib/mah-harness/sessions.db

# Windows (PowerShell)
mah start --grpc-port 50051 --http-port 50050 --store-path C:\ProgramData\mah-harness\sessions.db
```

这会建一个 SQLite 数据库存 session 状态。

---

## Linux

测试环境: **Ubuntu 22.04 / 24.04**, **Debian 12**, **Fedora 40**, **Arch** (rolling)。

### L1. 建专用用户

```bash
sudo useradd --system --shell /usr/sbin/nologin --home /var/lib/mah-harness mah
sudo mkdir -p /var/lib/mah-harness
sudo chown -R mah:mah /var/lib/mah-harness
sudo chmod 750 /var/lib/mah-harness
```

### L2. 装 `mah` 二进制

二选一:

```bash
# 方案 A: 从源码 (假设你已经有 install 文档里的 build 环境)
sudo install -m 0755 target/release/mah /usr/local/bin/mah

# 方案 B: 从 crates.io
sudo cargo install ma-harness-cli --root /usr/local
# 二进制在 /usr/local/bin/mah
```

### L3. systemd 服务

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

# 加固 (可选, 推荐)
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/mah-harness

[Install]
WantedBy=multi-user.target
```

启用 + 启动:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now mah-harness
sudo systemctl status mah-harness
```

日志去 journald:

```bash
sudo journalctl -u mah-harness -f    # follow
sudo journalctl -u mah-harness -n 200 --no-pager
```

### L4. nginx 反代 + HTTPS

```bash
sudo apt install -y nginx              # Debian / Ubuntu
sudo dnf install -y nginx              # Fedora
sudo pacman -S --needed nginx          # Arch
```

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

启用 + 重载:

```bash
sudo ln -s /etc/nginx/sites-available/mah-harness /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

### L5. Authentication (nginx basic auth)

```bash
# 建用户文件 (一次)
sudo apt install -y apache2-utils
sudo htpasswd -c /etc/nginx/.htpasswd myuser
# 后续加用户 (去掉 -c)
sudo htpasswd /etc/nginx/.htpasswd anotheruser
```

nginx 配置里:

```nginx
location / {
    auth_basic "mah-harness";
    auth_basic_user_file /etc/nginx/.htpasswd;

    proxy_pass http://mah_http;
    # ...
}
```

重载:

```bash
sudo nginx -t && sudo systemctl reload nginx
```

---

## macOS

测试环境: **macOS 14 Sonoma (Intel)**, **macOS 15 Sequoia (Apple Silicon)**。

macOS 用 **launchd** (不是 systemd)。下面路径走 Homebrew 约定;
Apple Silicon 上把 `/usr/local/var` 和 `/usr/local/etc` 换成
`/opt/homebrew/var` 和 `/opt/homebrew/etc`。

### M1. 装 nginx (via Homebrew)

```bash
brew install nginx
# Apple Silicon nginx 在 /opt/homebrew/etc/nginx
# Intel nginx 在 /usr/local/etc/nginx
```

> Caddy 也很好 (auto-HTTPS, 配置简单):
> `brew install caddy` — 看下面 [M5 章节](#m5-authentication-caddy)。

### M2. 建数据目录

```bash
# Apple Silicon
sudo mkdir -p /opt/homebrew/var/mah-harness
sudo chown -R $(whoami):admin /opt/homebrew/var/mah-harness

# Intel
sudo mkdir -p /usr/local/var/mah-harness
sudo chown -R $(whoami):admin /usr/local/var/mah-harness
```

### M3. launchd 服务

建 `~/Library/LaunchAgents/local.mah-harness.server.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>local.mah-harness.server</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/mah</string>     <!-- 或 /opt/homebrew/bin/mah -->
        <string>start</string>
        <string>--grpc-port</string>
        <string>50051</string>
        <string>--http-port</string>
        <string>50050</string>
        <string>--store-path</string>
        <string>/usr/local/var/mah-harness/sessions.db</string>
    </array>

    <key>WorkingDirectory</key>
    <string>/usr/local/var/mah-harness</string>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>

    <key>StandardOutPath</key>
    <string>/usr/local/var/log/mah-harness.log</string>
    <key>StandardErrorPath</key>
    <string>/usr/local/var/log/mah-harness.err</string>

    <key>SoftResourceLimits</key>
    <dict>
        <key>NumberOfFiles</key>
        <integer>65536</integer>
    </dict>
</dict>
</plist>
```

确认二进制在 `PATH` 上, 日志目录已建:

```bash
# 确认 plist 里写的二进制路径对
which mah
# /usr/local/bin/mah 或 /opt/homebrew/bin/mah

# 建日志目录
sudo mkdir -p /usr/local/var/log
sudo chown $(whoami):admin /usr/local/var/log
```

加载 + 启动:

```bash
# 加载 (自动启动; 下次登录也启动)
launchctl load ~/Library/LaunchAgents/local.mah-harness.server.plist

# 或者单次启动, 不开机自启
launchctl start local.mah-harness.server

# 验证
launchctl list | grep mah-harness
# 期望: <PID> 0 local.mah-harness.server
```

卸载 (停止开机自启):

```bash
launchctl unload ~/Library/LaunchAgents/local.mah-harness.server.plist
```

日志:

```bash
tail -f /usr/local/var/log/mah-harness.log
tail -f /usr/local/var/log/mah-harness.err
```

### M4. nginx 反代 + HTTPS (Homebrew)

编辑 `/usr/local/etc/nginx/nginx.conf` (Apple Silicon: `/opt/homebrew/etc/nginx/nginx.conf`)。
在 `http { }` 块里加 upstream + server:

```nginx
# http { } 块内:
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

    location / {
        proxy_pass http://mah_http;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location /mah_harness.v1.AgentService/ {
        grpc_pass grpc://mah_grpc;
    }
    location /mah_harness.v1.SessionService/ {
        grpc_pass grpc://mah_grpc;
    }
}
```

启动 nginx + 开机自启:

```bash
brew services start nginx
```

改完配置重载:

```bash
brew services restart nginx
# 或: nginx -t && nginx -s reload
```

### M5. Authentication (Caddy)

Caddy 在 macOS 上比 nginx 简单 — 自动 HTTPS via Let's Encrypt。

`brew install caddy`, 然后建 `/usr/local/etc/Caddyfile`:

```caddy
mah.example.com {
    # Authentication: HTTP Basic
    basicauth {
        myuser $2a$14$...    # 生成: caddy hash-password
    }

    # HTTP REST + OpenAPI
    reverse_proxy 127.0.0.1:50050

    # gRPC (Caddy 自动处理 HTTP/2)
    reverse_proxy /mah_harness.v1.* 127.0.0.1:50051
}
```

启动:

```bash
brew services start caddy
```

Caddyfile 改完重载:

```bash
brew services restart caddy
```

---

## Windows

测试环境: **Windows 10 21H2+**, **Windows 11**, **Windows Server 2019 / 2022**, **PowerShell 5.1 / 7.x**。

Windows 没有 systemd。跑长寿命 exe 的标准方法是 **Windows Service** 基础设施,
用 **NSSM** (推荐) 或 **sc.exe** (内置, 但有限制) 包装。

### W1. 装 NSSM (服务包装器)

NSSM 是个小工具, 把任何 `.exe` 注册成 Windows Service, 带自动重启 + 日志轮转 + 依赖管理。

```powershell
# 方案 A: winget
winget install --id=NSSM.NSSM -e

# 方案 B: choco
choco install nssm

# 方案 C: scoop
scoop install nssm

# 验证
nssm --version
```

如果都不行, 从 <https://nssm.cc/download> 手动下载, 把 `nssm.exe` 放到 `PATH`。

### W2. 建数据目录

```powershell
$dataDir = "C:\ProgramData\mah-harness"
New-Item -ItemType Directory -Path $dataDir -Force
icacls $dataDir /inheritance:r
icacls $dataDir /grant:r "SYSTEM:(OI)(CI)F"
icacls $dataDir /grant:r "Administrators:(OI)(CI)F"
# 后面要给非管理员服务账号权限, 加:
# icacls $dataDir /grant:r "your-svc-user:(OI)(CI)M"
```

> `C:\ProgramData` 是非用户特定应用数据的标准位置。
> 上面 `icacls` 锁住权限, 防止非管理员读 SQLite session 数据库。

### W3. 注册 Windows Service

```powershell
# 用管理员身份跑 PowerShell
$nssm = (Get-Command nssm).Source
$exe  = "$env:USERPROFILE\.cargo\bin\mah.exe"

# 验证二进制存在
Test-Path $exe   # 应该 True

# 安装 (注册服务; 第一次跑会启动)
nssm install mah-harness $exe `
    "start --grpc-port 50051 --http-port 50050 --store-path C:\ProgramData\mah-harness\sessions.db"

# 设工作目录
nssm set mah-harness AppDirectory "C:\ProgramData\mah-harness"

# 自动重启 (crash 后等 5 秒)
nssm set mah-harness AppExit Default Restart
nssm set mah-harness AppRestartDelay 5000

# 日志: stdout + stderr → 文件, 10 MB 轮转
New-Item -ItemType Directory -Path "$env:ProgramData\mah-harness\logs" -Force
nssm set mah-harness AppStdoutFile "$env:ProgramData\mah-harness\logs\stdout.log"
nssm set mah-harness AppStderrFile "$env:ProgramData\mah-harness\logs\stderr.log"
nssm set mah-harness AppStdoutCreationDisposition 4   # Append
nssm set mah-harness AppRotateFiles 1
nssm set mah-harness AppRotateBytes 10485760           # 10 MB

# 启动
Start-Service mah-harness
```

验证:

```powershell
# 服务状态
Get-Service mah-harness
# 期望: Status = Running

# 最近日志
Get-Content "$env:ProgramData\mah-harness\logs\stdout.log" -Tail 10
```

管理服务:

```powershell
Stop-Service mah-harness
Start-Service mah-harness
Restart-Service mah-harness

# 完全删除
nssm remove mah-harness confirm
```

> **排错**: 服务起不来, 看 Windows 事件查看器 → Windows 日志 → 应用程序里的 NSSM 错误。
> 常见原因: `mah.exe` 路径错了, 或 `--store-path` 目录不存在 / 服务账号写不进去。

### W4. 反代 + HTTPS

三个合理选项, 选一个:

| 反代 | 优点 | 缺点 |
|---|---|---|
| **IIS + ARR** (Application Request Routing) | Windows Server 原生 | 配置复杂, gRPC 要 HTTP/2 + buffering 调 |
| **nginx for Windows** | 跟 Linux 同 config, 大家熟 | 不是官方 Windows build (社区 port) |
| **Caddy** | 自动 HTTPS, 配置简单 | Windows 管理员不太熟 |

#### W4a. nginx for Windows

1. 从 <https://nginx.org/en/download.html> 下载, 解压到 `C:\nginx\`
   (e.g. `C:\nginx-1.27.4\`)。
2. 编辑 `C:\nginx\conf\nginx.conf`, 加上 [Linux nginx 章节](#l4-nginx-https) 里
   的 upstream + server 块。
3. 普通 PowerShell 启 (改 listen 端口 8080/8443 不用管理员):

   ```powershell
   cd C:\nginx
   Start-Process -FilePath .\nginx.exe
   ```

4. **注意**: nginx for Windows 是单进程 worker; 高并发 gRPC 场景
   需要调 `worker_connections`。

#### W4b. Caddy (自动 HTTPS)

```powershell
# 装: https://caddyserver.com/docs/install#windows
# 装完编辑 C:\Users\<you>\AppData\Roaming\Caddy\Caddyfile
```

```caddy
mah.example.com {
    basicauth {
        myuser $2a$14$...    # caddy hash-password
    }

    reverse_proxy 127.0.0.1:50050
    reverse_proxy /mah_harness.v1.* 127.0.0.1:50051
}
```

然后用 NSSM (跟 W3 同) 把 Caddy 本身也注册成 Windows Service,
或用 Task Scheduler 开机跑。

#### W4c. Cloudflare Tunnel (零配置 HTTPS, 不开端口)

如果 Windows server 在防火墙后面, **Cloudflare Tunnel** 是最简单的路 —
不用开端口, 自动 HTTPS, 可选 Zero-Trust auth。

1. 装 `cloudflared`: <https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/>
2. 登录: `cloudflared tunnel login`
3. 建 tunnel: `cloudflared tunnel create mah-harness`
4. 配置 `~/.cloudflared/config.yml`:

   ```yaml
   tunnel: <TUNNEL_ID>
   credentials-file: C:\Users\<you>\.cloudflared\<TUNNEL_ID>.json

   ingress:
     - hostname: mah.example.com
       service: http://127.0.0.1:50050
     - service: http_status:404
   ```

5. 注册成 Windows Service: `cloudflared service install`

Auth + gRPC 走 Cloudflare 自动搞定。

---

## 从其他机器 gRPC 客户端 (三个系统通用)

服务起来 + 可达 (上面三个 OS 任何一个) 之后:

```bash
# 装 grpcurl
# Linux: snap install grpcurl
# macOS: brew install grpcurl
# Windows: scoop install grpcurl (或 winget install FullStory.Grpcurl)

# 列服务
grpcurl -insecure mah.example.com:443 list
# 期望: mah_harness.v1.AgentService, mah_harness.v1.SessionService

# 调方法
grpcurl -insecure -d '{"session_id": "demo", "message": "hello"}' \
  mah.example.com:443 mah_harness.v1.AgentService/Run
```

或用 `mah` 本身 (P11+ `RunStream`):

```bash
mah run-stream --grpc-url https://mah.example.com:443 "hello"
```

## 验证 (三个系统通用)

```bash
# Health (HTTP)
curl https://mah.example.com/health
# 期望: 200

# OpenAPI spec 可用
curl https://mah.example.com/openapi.json | jq .info.title
# 期望: "ma-harness.rs"

# gRPC 过反代工作
grpcurl -insecure mah.example.com:443 list
```

## 资源规划 (三个系统通用)

| 负载 | CPU | 内存 | 磁盘 (每 1M 事件) |
|---|---|---|---|
| 轻 (< 10 req/min) | 1 core | 512 MB | ~200 MB |
| 中 (10-100 req/min) | 2 cores | 2 GB | ~2 GB |
| 重 (100-1000 req/min) | 4 cores | 8 GB | ~20 GB |

Event log 是 append-only SQLite。每月 vacuum:

```bash
# Linux / macOS
sqlite3 /var/lib/mah-harness/sessions.db VACUUM;

# Windows (PowerShell)
& "$env:ProgramFiles\sqlite\sqlite3.exe" "C:\ProgramData\mah-harness\sessions.db" "VACUUM;"
```

## 下一步

- 加 **插件** 提供真实能力 — 见 [04-plugins.md](04-plugins.md)
- **验证** 部署 — 见 [05-conformance.md](05-conformance.md)
- **扩展** 多 server: gRPC 无状态; session 用共享 Postgres (P12+)

## Troubleshooting

### 启动报 "address already in use"

别的进程占着端口。找它:

```bash
# Linux
sudo lsof -i :50051
# 或: sudo ss -tlnp 'sport = :50051'

# macOS
sudo lsof -iTCP:50051 -sTCP:LISTEN

# Windows (PowerShell)
Get-NetTCPConnection -LocalPort 50051 -State Listen
```

### gRPC 过 nginx: "connection reset by peer"

nginx 的 listen 指令需要 `http2` (HTTP/2 cleartext → gRPC):

```nginx
listen 443 ssl http2;   # ← http2 必填
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

太多就定期重启:

```ini
# Linux systemd
# 加到 mah-harness.service:
RestartSec=300
```

```xml
<!-- macOS launchd plist: 调 throttle interval -->
<key>ThrottleInterval</key>
<integer>300</integer>
```

```powershell
# Windows NSSM
nssm set mah-harness AppRestartDelay 300000   # 5 分钟
```

### Linux: `systemctl status` 显示 "failed"

```bash
sudo journalctl -u mah-harness -n 50 --no-pager
# 常见原因: --store-path 目录写不进去
sudo chown -R mah:mah /var/lib/mah-harness
```

### macOS: `launchctl load` 报 "service already loaded"

```bash
launchctl unload ~/Library/LaunchAgents/local.mah-harness.server.plist
launchctl load ~/Library/LaunchAgents/local.mah-harness.server.plist
```

### Windows: 服务起不来, 事件查看器有 error

```powershell
# 看 NSSM 报告什么
nssm status mah-harness

# 重查 exe 路径
nssm get mah-harness Application

# 直接跑二进制测试
& "$env:USERPROFILE\.cargo\bin\mah.exe" start --grpc-port 50051 --http-port 50050
# 能跑起来就是服务账号 / 工作目录的问题
```

常见 Windows 问题:

- **AppDirectory 缺失** → `nssm set mah-harness AppDirectory C:\ProgramData\mah-harness`
- **store-path 写不进去** → 重跑 W2 的 `icacls` 给服务账号授权
- **端口被占** → `Get-NetTCPConnection -LocalPort 50051` 查冲突
