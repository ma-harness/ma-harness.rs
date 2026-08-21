# scripts/publish-v0.1.1.ps1
#
# 2026-08-21 (Day 101+2): v0.1.1 release 手动 publish 脚本
# 原因: GitHub Actions 账户额度用完, release workflow 没法跑
# 业务方本机手动 cargo publish, 按 tier 顺序 (跟 release.yml 一样)
# 每 crate 后 sleep 30s 等 crates.io index 同步
#
# 准备:
#   1. 装 protoc: scoop install protobuf (业务方用的是 scoop, 不是 choco)
#      或: choco install -y protoc (需 admin)
#   2. cargo login (拿 token from crates.io/settings/tokens, scope: publish)
#      或: $env:CARGO_REGISTRY_TOKEN = "<token>" (本脚本自动检测)
#   3. 验证: protoc --version  +  cat ~/.cargo/credentials.toml
#
# 用法:
#   cd D:\workspace\learn\rust\ma-harness.rs
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\publish-v0.1.1.ps1
#
# 输出: 17 行 "OK" / "FAIL" + 17 * 30s sleep = ~10-15 min

$ErrorActionPreference = 'Continue'
$SleepSec = 30

# 2026-08-21 (Day 101+2): 业务方用 scoop 装 protoc, scoop\shims 不在 PATH
# 自动加 (业务方本机唯一确认装好的路径)
$scoopShims = 'C:\Users\Administrator\scoop\shims'
if (Test-Path $scoopShims) {
  $env:PATH = "$scoopShims;$env:PATH"
  Write-Host "Added $scoopShims to PATH for this session" -ForegroundColor DarkGray
}
$protoc = Get-Command protoc -ErrorAction SilentlyContinue
if (-not $protoc) {
  Write-Host "ERROR: protoc not found. Install: scoop install protobuf" -ForegroundColor Red
  exit 1
}
Write-Host "protoc: $(protoc --version)" -ForegroundColor DarkGray

# 2026-08-21 (Day 101+2): 验证 cargo login (有 credentials.toml 或 env var)
if (-not (Test-Path ~/.cargo/credentials.toml)) {
  if (-not $env:CARGO_REGISTRY_TOKEN) {
    Write-Host "ERROR: cargo not logged in. Run: cargo login <token>" -ForegroundColor Red
    Write-Host "  or: `$env:CARGO_REGISTRY_TOKEN = '<token>'" -ForegroundColor Red
    Write-Host "  Get token from: https://crates.io/settings/tokens (scope: publish)" -ForegroundColor Red
    exit 1
  } else {
    Write-Host "Using CARGO_REGISTRY_TOKEN env var (cargo login not done)" -ForegroundColor Yellow
  }
} else {
  Write-Host "cargo credentials: $(Test-Path ~/.cargo/credentials.toml)" -ForegroundColor DarkGray
}
$TotalOk = 0
$TotalFail = 0

# 17 crate 按 tier 顺序 (不依赖 proto 的 14 个, 依赖 proto 的 3 个)
# 平铺 + tier 注释代替 ordered hash 嵌套 (PowerShell 5.1 兼容)
$PublishList = @(
  @{ Tier = 'Tier 2';   Crate = 'ma-harness-model' },
  @{ Tier = 'Tier 2';   Crate = 'ma-harness-plugin-macro' },
  @{ Tier = 'Tier 3';   Crate = 'ma-harness-seam' },
  @{ Tier = 'Tier 3';   Crate = 'ma-harness-bundle' },
  @{ Tier = 'Tier 3';   Crate = 'ma-harness-conformance' },
  @{ Tier = 'Tier 3.5'; Crate = 'ma-harness-plugin-hello' },
  @{ Tier = 'Tier 3.5'; Crate = 'ma-harness-plugin-bash' },
  @{ Tier = 'Tier 3.5'; Crate = 'ma-harness-plugin-cordis' },
  @{ Tier = 'Tier 3.5'; Crate = 'ma-harness-plugin-fs' },
  @{ Tier = 'Tier 3.5'; Crate = 'ma-harness-plugin-skill' },
  @{ Tier = 'Tier 3.5'; Crate = 'ma-harness-plugin-subagent' },
  @{ Tier = 'Tier 3.5'; Crate = 'ma-harness-plugin-web' },
  @{ Tier = 'Tier 3.5'; Crate = 'ma-harness-plugin-dsh-adapter' },
  @{ Tier = 'Tier 4';   Crate = 'ma-harness-server' },
  @{ Tier = 'Tier 5';   Crate = 'ma-harness-tui' },
  @{ Tier = 'Tier 5';   Crate = 'ma-harness-cli' }
)

# 已发 0.1.0 / 0.1.1 跳过 (跟 release.yml 一致)
$AlreadyPublished = @(
  'ma-harness-cordis', 'ma-harness-code', 'ma-harness-dag',
  'ma-harness-artifact', 'ma-harness-core',
  'ma-harness-sandbox', 'ma-harness-proto', 'ma-harness-registry'
)

$CurrentTier = ''
foreach ($item in $PublishList) {
  $tier = $item.Tier
  $crate = $item.Crate

  if ($tier -ne $CurrentTier) {
    Write-Host ''
    Write-Host "===== $tier =====" -ForegroundColor Cyan
    $CurrentTier = $tier
  }

  if ($AlreadyPublished -contains $crate) {
    Write-Host "  $crate : already published, skip" -ForegroundColor Yellow
    continue
  }

  Write-Host "  $crate : publishing ..." -ForegroundColor White
  # cargo publish: --no-verify 跳过 build verification (test job 已验过)
  #                --allow-dirty 允许 working tree 不干净
  #                --registry crates-io 强制 crates.io 源 (rsproxy 国内镜像不能 publish)
  $output = cargo publish -p $crate --no-verify --allow-dirty --registry crates-io 2>&1
  $exitCode = $LASTEXITCODE
  $output | Select-Object -Last 5 | ForEach-Object { Write-Host "    $_" }
  if ($exitCode -eq 0) {
    Write-Host "  $crate : OK, wait ${SleepSec}s for index sync ..." -ForegroundColor Green
    Start-Sleep -Seconds $SleepSec
    $TotalOk++
  } else {
    Write-Host "  $crate : FAIL (exit $exitCode)" -ForegroundColor Red
    $TotalFail++
  }
}

Write-Host ''
Write-Host '===== 总结 =====' -ForegroundColor Cyan
Write-Host "Published OK:    $TotalOk"
Write-Host "Published FAIL:  $TotalFail"
Write-Host "Already done:    $($AlreadyPublished.Count)"
Write-Host "Total:           $($TotalOk + $TotalFail + $AlreadyPublished.Count) / 24"
