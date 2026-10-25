# 启动 Tauri 开发模式。Rust 安装器默认把 cargo 放在用户目录，显式补进 PATH，
# 避免刚装完 Rust 的终端里 Tauri CLI 找不到 cargo。
$ErrorActionPreference = "Stop"
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) {
    $env:Path = "$cargoBin;$env:Path"
}
pnpm tauri dev
