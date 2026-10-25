param(
    [switch]$SkipTests
)

# One-shot release: tests -> frontend build -> Tauri release build -> copy artifacts.
$ErrorActionPreference = "Continue"

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) {
    $env:Path = "$cargoBin;$env:Path"
}

if (-not $SkipTests) {
    Write-Host "==> Rust tests"
    cargo test --manifest-path src-tauri/Cargo.toml --lib
    if ($LASTEXITCODE -ne 0) { throw "Rust tests failed" }

    Write-Host "==> TypeScript check"
    pnpm check
    if ($LASTEXITCODE -ne 0) { throw "TypeScript check failed" }
}

Write-Host "==> Tauri release build"
pnpm tauri build
if ($LASTEXITCODE -ne 0) { throw "Tauri build failed" }

$releaseDir = Join-Path $root "release"
New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null

$exeSource = Join-Path $root "src-tauri\target\release\leon-redis-client.exe"
if (-not (Test-Path $exeSource)) { throw "Release exe not found: $exeSource" }
$exeTarget = Join-Path $releaseDir "LeonRedis.exe"
try {
    Copy-Item -LiteralPath $exeSource -Destination $exeTarget -Force -ErrorAction Stop
} catch {
    throw "无法写入 $exeTarget。请先关闭正在运行的 LeonRedis.exe 后重试。$($_.Exception.Message)"
}

$bundleDir = Join-Path $root "src-tauri\target\release\bundle\nsis"
$setup = Get-ChildItem $bundleDir -Filter "*-setup.exe" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if ($setup) {
    $setupTarget = Join-Path $releaseDir $setup.Name
    try {
        Copy-Item -LiteralPath $setup.FullName -Destination $setupTarget -Force -ErrorAction Stop
    } catch {
        throw "无法写入 $setupTarget。请检查文件是否被占用。$($_.Exception.Message)"
    }
}

Write-Host ""
Write-Host "Release artifacts:"
Write-Host ("  " + $exeTarget)
if ($setup) {
    Write-Host ("  " + $setupTarget)
}
