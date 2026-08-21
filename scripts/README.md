# scripts/ — 手动 publish 脚本

## `publish-v0.1.1.ps1`

2026-08-21 v0.1.1 release 手动 publish 脚本（GitHub Actions 账户额度用完时用）。

### 准备

```powershell
# 1. 装 protoc (需要管理员 PowerShell)
choco install -y protoc

# 2. 拿 crates.io API token
# 打开 https://crates.io/settings/tokens
# 点 "New Token" → name "ma-harness release" → scope "Publish"
# 复制 token (一次性显示)

# 3. cargo login
cargo login <粘贴 token>

# 4. 验证
protoc --version      # 应输出 libprotoc 3.21.x
cargo whoami          # 应输出 i25ma
```

### 跑

```powershell
cd D:\workspace\learn\rust\ma-harness.rs
pwsh scripts\publish-v0.1.1.ps1
```

### 行为

- 按 tier 顺序 (Tier 2 → 3 → 3.5 → 4 → 5) 跑 17 个 `cargo publish`
- 已发 7 个 (cordis/code/dag/artifact/core 0.1.0 + sandbox/proto/registry 0.1.1) 自动 skip
- 每个 crate 后 sleep 30s 让 crates.io index 同步
- 预计 10-15 分钟跑完

### Crate 分类

**不依赖 ma-harness-proto (14 个, 装 protoc 前能发):**
- Tier 2: model, plugin-macro
- Tier 3: seam, bundle, conformance
- Tier 3.5: 8 plugins (hello/bash/cordis/fs/skill/subagent/web/dsh-adapter)

**依赖 ma-harness-proto (3 个, 需装 protoc):**
- Tier 4: server
- Tier 5: tui, cli
