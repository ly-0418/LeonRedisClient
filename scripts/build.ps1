# 构建 Windows 安装包。输出目录：src-tauri/target/release/bundle/
$ErrorActionPreference = "Stop"
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) {
    $env:Path = "$cargoBin;$env:Path"
}
pnpm tauri build
