# 03 — Server Deployment

> **Goal**: run `mah start` as a long-lived server with gRPC + HTTP,
> behind a reverse proxy, with HTTPS and basic auth.
> Pick your OS and jump to the matching deployment section.

[English](03-server.md) | [简体中文](../../zh-CN/user-guide/03-server.md)

## Pick your OS

| OS | Service manager | Reverse proxy | Jump to |
|---|---|---|---|
| **Linux** (Debian / Ubuntu / Fedora / Arch) | systemd | nginx | [Linux deploy](#linux) |
| **macOS** (Sonoma / Sequoia) | launchd | nginx or Caddy (via Homebrew) | [macOS deploy](#macos) |
| **Windows** (10 / 11, Server 2019/2022) | Windows Service (NSSM / sc.exe) | IIS ARR or nginx | [Windows deploy](#windows) |

If you only need to **try it on localhost** first, jump to
[Quick smoke test](#quick-smoke-test-all-oses) — it works on all three OSes.

## Common prerequisites (all OSes)

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

## Quick smoke test (all OSes)

Before setting up a service manager or reverse proxy, verify `mah start` works on your box:

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

To stop the foreground process, press **Ctrl-C**. If you backgrounded it with `&`,
kill it with `kill %1` (Linux/macOS) or `Stop-Process -Id $PID` (Windows PowerShell).

## Persistent session storage

By default, sessions are in-memory (lost on restart). To persist, point `--store-path`
at a writable location:

| OS | Recommended path |
|---|---|
| **Linux** | `/var/lib/mah-harness/sessions.db` |
| **macOS** | `/usr/local/var/mah-harness/sessions.db` (Intel) or `/opt/homebrew/var/mah-harness/sessions.db` (Apple Silicon) |
| **Windows** | `C:\ProgramData\mah-harness\sessions.db` |

Create the directory first (each OS section has the exact commands), then:

```bash
# Linux / macOS
mah start \
  --grpc-port 50051 \
  --http-port 50050 \
  --store-path /var/lib/mah-harness/sessions.db

# Windows (PowerShell)
mah start --grpc-port 50051 --http-port 50050 --store-path C:\ProgramData\mah-harness\sessions.db
```

This creates a SQLite database for session state.

---

## Linux

Tested on: **Ubuntu 22.04 / 24.04**, **Debian 12**, **Fedora 40**, **Arch** (rolling).

### L1. Create a dedicated user

```bash
sudo useradd --system --shell /usr/sbin/nologin --home /var/lib/mah-harness mah
sudo mkdir -p /var/lib/mah-harness
sudo chown -R mah:mah /var/lib/mah-harness
sudo chmod 750 /var/lib/mah-harness
```

### L2. Install the `mah` binary

Pick one:

```bash
# Option A: from source (assumes you have the build env from install doc)
sudo install -m 0755 target/release/mah /usr/local/bin/mah

# Option B: from crates.io
sudo cargo install ma-harness-cli --root /usr/local
# Binary at /usr/local/bin/mah
```

### L3. systemd service

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

# Hardening (optional but recommended)
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/mah-harness

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now mah-harness
sudo systemctl status mah-harness
```

Logs go to journald:

```bash
sudo journalctl -u mah-harness -f    # follow
sudo journalctl -u mah-harness -n 200 --no-pager
```

### L4. nginx reverse proxy + HTTPS

```bash
sudo apt install -y nginx              # Debian / Ubuntu
sudo dnf install -y nginx              # Fedora
sudo pacman -S --needed nginx          # Arch
```

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

### L5. Authentication (nginx basic auth)

```bash
# Create user file (one-time)
sudo apt install -y apache2-utils
sudo htpasswd -c /etc/nginx/.htpasswd myuser
# Add more users later (omit -c):
sudo htpasswd /etc/nginx/.htpasswd anotheruser
```

In the nginx config:

```nginx
location / {
    auth_basic "mah-harness";
    auth_basic_user_file /etc/nginx/.htpasswd;

    proxy_pass http://mah_http;
    # ...
}
```

Reload:

```bash
sudo nginx -t && sudo systemctl reload nginx
```

---

## macOS

Tested on: **macOS 14 Sonoma (Intel)**, **macOS 15 Sequoia (Apple Silicon)**.

macOS uses **launchd** (not systemd). All paths below use Homebrew conventions;
on Apple Silicon replace `/usr/local/var` and `/usr/local/etc` with
`/opt/homebrew/var` and `/opt/homebrew/etc`.

### M1. Install nginx (via Homebrew)

```bash
brew install nginx
# Apple Silicon nginx lives at /opt/homebrew/etc/nginx
# Intel nginx lives at /usr/local/etc/nginx
```

> Caddy is also great (auto-HTTPS, simpler config):
> `brew install caddy` — see the [Caddy section](#m5-authentication-caddy) below.

### M2. Create a data directory

```bash
# Apple Silicon
sudo mkdir -p /opt/homebrew/var/mah-harness
sudo chown -R $(whoami):admin /opt/homebrew/var/mah-harness

# Intel
sudo mkdir -p /usr/local/var/mah-harness
sudo chown -R $(whoami):admin /usr/local/var/mah-harness
```

### M3. launchd service

Create `~/Library/LaunchAgents/local.mah-harness.server.plist`:

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
        <string>/usr/local/bin/mah</string>     <!-- or /opt/homebrew/bin/mah -->
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

Make sure the binary is on `PATH` and the log directory exists:

```bash
# Confirm the binary path you used in the plist is correct
which mah
# /usr/local/bin/mah or /opt/homebrew/bin/mah

# Create log dir
sudo mkdir -p /usr/local/var/log
sudo chown $(whoami):admin /usr/local/var/log
```

Load and start:

```bash
# Load (auto-starts; will also start on next login)
launchctl load ~/Library/LaunchAgents/local.mah-harness.server.plist

# Or start once without auto-start
launchctl start local.mah-harness.server

# Verify
launchctl list | grep mah-harness
# Expect: <PID> 0 local.mah-harness.server
```

Unload to stop auto-start:

```bash
launchctl unload ~/Library/LaunchAgents/local.mah-harness.server.plist
```

Logs:

```bash
tail -f /usr/local/var/log/mah-harness.log
tail -f /usr/local/var/log/mah-harness.err
```

### M4. nginx reverse proxy + HTTPS (Homebrew)

Edit `/usr/local/etc/nginx/nginx.conf` (Apple Silicon: `/opt/homebrew/etc/nginx/nginx.conf`).
In the `http { }` block, add the upstream + server block:

```nginx
# Inside the http { } block:
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

Start nginx and enable at boot:

```bash
brew services start nginx
```

Reload after config changes:

```bash
brew services restart nginx
# or: nginx -t && nginx -s reload
```

### M5. Authentication (Caddy)

Caddy is simpler than nginx on macOS — automatic HTTPS via Let's Encrypt.

`brew install caddy` then create `/usr/local/etc/Caddyfile`:

```caddy
mah.example.com {
    # Authentication: HTTP Basic
    basicauth {
        myuser $2a$14$...    # generate with: caddy hash-password
    }

    # HTTP REST + OpenAPI
    reverse_proxy 127.0.0.1:50050

    # gRPC (Caddy handles HTTP/2 automatically)
    reverse_proxy /mah_harness.v1.* 127.0.0.1:50051
}
```

Start:

```bash
brew services start caddy
```

Reload after Caddyfile changes:

```bash
brew services restart caddy
```

---

## Windows

Tested on: **Windows 10 21H2+**, **Windows 11**, **Windows Server 2019 / 2022**, **PowerShell 5.1 / 7.x**.

Windows has no systemd. The standard way to run a long-lived executable is the
**Windows Service** infrastructure, wrapped via **NSSM** (recommended) or
**sc.exe** (built-in but limited).

### W1. Install NSSM (the service wrapper)

NSSM is a tiny utility that registers any `.exe` as a Windows Service with
auto-restart, log rotation, and dependency management.

```powershell
# Option A: winget
winget install --id=NSSM.NSSM -e

# Option B: choco
choco install nssm

# Option C: scoop
scoop install nssm

# Verify
nssm --version
```

If none of those work, download from <https://nssm.cc/download> and put
`nssm.exe` on `PATH`.

### W2. Create data directory

```powershell
$dataDir = "C:\ProgramData\mah-harness"
New-Item -ItemType Directory -Path $dataDir -Force
icacls $dataDir /inheritance:r
icacls $dataDir /grant:r "SYSTEM:(OI)(CI)F"
icacls $dataDir /grant:r "Administrators:(OI)(CI)F"
# Allow a non-admin service account later if needed:
# icacls $dataDir /grant:r "your-svc-user:(OI)(CI)M"
```

> `C:\ProgramData` is the canonical location for non-user-specific app data.
> `icacls` above locks it down so non-admin users can't read the SQLite
> session database.

### W3. Register the Windows Service

```powershell
# Run PowerShell as Administrator
$nssm = (Get-Command nssm).Source
$exe  = "$env:USERPROFILE\.cargo\bin\mah.exe"

# Verify the binary exists
Test-Path $exe   # should be True

# Install (registers the service; starts it on first run)
nssm install mah-harness $exe `
    "start --grpc-port 50051 --http-port 50050 --store-path C:\ProgramData\mah-harness\sessions.db"

# Set working directory
nssm set mah-harness AppDirectory "C:\ProgramData\mah-harness"

# Auto-restart on crash (delay 5s, then up to 3 restarts)
nssm set mah-harness AppExit Default Restart
nssm set mah-harness AppRestartDelay 5000

# Logs: stdout + stderr → files with rotation at 10 MB
New-Item -ItemType Directory -Path "$env:ProgramData\mah-harness\logs" -Force
nssm set mah-harness AppStdoutFile "$env:ProgramData\mah-harness\logs\stdout.log"
nssm set mah-harness AppStderrFile "$env:ProgramData\mah-harness\logs\stderr.log"
nssm set mah-harness AppStdoutCreationDisposition 4   # Append
nssm set mah-harness AppRotateFiles 1
nssm set mah-harness AppRotateBytes 10485760           # 10 MB

# Start it
Start-Service mah-harness
```

Verify:

```powershell
# Service status
Get-Service mah-harness
# Expect: Status = Running

# Last few log lines
Get-Content "$env:ProgramData\mah-harness\logs\stdout.log" -Tail 10
```

Manage the service:

```powershell
Stop-Service mah-harness
Start-Service mah-harness
Restart-Service mah-harness

# Remove entirely
nssm remove mah-harness confirm
```

> **Troubleshooting**: if the service won't start, check the Windows Event
> Viewer → Windows Logs → Application for the NSSM error message. Common
> cause: the `mah.exe` path is wrong, or the `--store-path` directory
> doesn't exist or isn't writable by the service account.

### W4. Reverse proxy + HTTPS

You have three reasonable options. Pick one:

| Proxy | Pros | Cons |
|---|---|---|
| **IIS + ARR** (Application Request Routing) | Native to Windows Server | More complex setup, gRPC needs HTTP/2 + buffering tweaks |
| **nginx for Windows** | Same config as Linux, well-understood | Not an official Windows build (community port) |
| **Caddy** | Auto-HTTPS, simple config | Newer to most Windows admins |

#### W4a. nginx for Windows

1. Download from <https://nginx.org/en/download.html> → extract to
   `C:\nginx\` (e.g. `C:\nginx-1.27.4\`).
2. Edit `C:\nginx\conf\nginx.conf` and add the upstream + server block
   from the [Linux nginx section](#l4-nginx-reverse-proxy-https) above.    
3. Start nginx from a normal PowerShell (no admin needed if you change
   the listen port to 8080/8443):

   ```powershell
   cd C:\nginx
   Start-Process -FilePath .\nginx.exe
   ```

4. **Important**: nginx for Windows is a single-process worker; under
   high gRPC concurrency you may need to tune `worker_connections`.

#### W4b. Caddy (auto-HTTPS)

```powershell
# Install: see https://caddyserver.com/docs/install#windows
# After install, edit C:\Users\<you>\AppData\Roaming\Caddy\Caddyfile
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

Then register Caddy itself as a Windows Service using NSSM (same as W3),
or run via Task Scheduler at boot.

#### W4c. Cloudflare Tunnel (zero-config HTTPS, no port forwarding)

If your Windows server is behind a firewall, **Cloudflare Tunnel** is the
easiest path — no open ports, automatic HTTPS, optional Zero-Trust auth.

1. Install `cloudflared`: <https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/>
2. Login: `cloudflared tunnel login`
3. Create a tunnel: `cloudflared tunnel create mah-harness`
4. Configure `~/.cloudflared/config.yml`:

   ```yaml
   tunnel: <TUNNEL_ID>
   credentials-file: C:\Users\<you>\.cloudflared\<TUNNEL_ID>.json

   ingress:
     - hostname: mah.example.com
       service: http://127.0.0.1:50050
     - service: http_status:404
   ```

5. Run as a Windows Service: `cloudflared service install`

Auth + gRPC work automatically through Cloudflare.

---

## gRPC client from another machine (all OSes)

Once your server is up and reachable (any of the three OSes above):

```bash
# Install grpcurl
# Linux: snap install grpcurl
# macOS: brew install grpcurl
# Windows: scoop install grpcurl (or winget install FullStory.Grpcurl)

# List services
grpcurl -insecure mah.example.com:443 list
# Expected: mah_harness.v1.AgentService, mah_harness.v1.SessionService

# Call a method
grpcurl -insecure -d '{"session_id": "demo", "message": "hello"}' \
  mah.example.com:443 mah_harness.v1.AgentService/Run
```

Or with `mah` itself (P11+ `RunStream`):

```bash
mah run-stream --grpc-url https://mah.example.com:443 "hello"
```

## Verify (all OSes)

```bash
# Health (HTTP)
curl https://mah.example.com/health
# Expected: 200

# OpenAPI spec available
curl https://mah.example.com/openapi.json | jq .info.title
# Expected: "ma-harness.rs"

# gRPC works through reverse proxy
grpcurl -insecure mah.example.com:443 list
```

## Resource planning (all OSes)

| Workload | CPU | RAM | Disk (per 1M events) |
|---|---|---|---|
| Light (< 10 req/min) | 1 core | 512 MB | ~200 MB |
| Medium (10-100 req/min) | 2 cores | 2 GB | ~2 GB |
| Heavy (100-1000 req/min) | 4 cores | 8 GB | ~20 GB |

The event log is append-only SQLite. Vacuum monthly:

```bash
# Linux / macOS
sqlite3 /var/lib/mah-harness/sessions.db VACUUM;

# Windows (PowerShell)
& "$env:ProgramFiles\sqlite\sqlite3.exe" "C:\ProgramData\mah-harness\sessions.db" "VACUUM;"
```

## What's next

- Add **plugins** for real capabilities — see [04-plugins.md](04-plugins.md)
- **Validate** your deployment with conformance tests — see [05-conformance.md](05-conformance.md)
- **Scale** beyond a single server: gRPC is stateless; sessions can be
  in a shared Postgres (P12+)

## Troubleshooting

### "address already in use" on startup

Another process is using the port. Find it:

```bash
# Linux
sudo lsof -i :50051
# or: sudo ss -tlnp 'sport = :50051'

# macOS
sudo lsof -iTCP:50051 -sTCP:LISTEN

# Windows (PowerShell)
Get-NetTCPConnection -LocalPort 50051 -State Listen
```

### gRPC over nginx: "connection reset by peer"

nginx needs `http2` on the listen directive (for HTTP/2 cleartext → gRPC):

```nginx
listen 443 ssl http2;   # ← http2 is required
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
# Linux systemd
# Add to mah-harness.service:
RestartSec=300
```

```xml
<!-- macOS launchd plist: bump throttle interval -->
<key>ThrottleInterval</key>
<integer>300</integer>
```

```powershell
# Windows NSSM
nssm set mah-harness AppRestartDelay 300000   # 5 min
```

### Linux: `systemctl status` shows "failed"

```bash
sudo journalctl -u mah-harness -n 50 --no-pager
# Common cause: --store-path directory not writable
sudo chown -R mah:mah /var/lib/mah-harness
```

### macOS: `launchctl load` fails with "service already loaded"

```bash
launchctl unload ~/Library/LaunchAgents/local.mah-harness.server.plist
launchctl load ~/Library/LaunchAgents/local.mah-harness.server.plist
```

### Windows: service won't start, Event Viewer shows error

```powershell
# Check what NSSM reports
nssm status mah-harness

# Re-check the exe path
nssm get mah-harness Application

# Test running the binary directly
& "$env:USERPROFILE\.cargo\bin\mah.exe" start --grpc-port 50051 --http-port 50050
# If this works, the issue is the service account or working directory.
```

Common Windows issues:

- **AppDirectory missing** → `nssm set mah-harness AppDirectory C:\ProgramData\mah-harness`
- **store-path not writable** → re-run W2's `icacls` to grant the service account
- **Port already in use** → `Get-NetTCPConnection -LocalPort 50051` to find the conflict
