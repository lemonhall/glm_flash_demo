#!/usr/bin/env pwsh
param(
    [switch]$Release,
    [switch]$Install,
    [string]$Distribution = "Ubuntu-24.04"
)

Write-Host "🐧 使用 WSL2 编译 DeepSeek Proxy" -ForegroundColor Cyan

# 检查 WSL 是否可用
if (-not (Get-Command wsl -ErrorAction SilentlyContinue)) {
    Write-Host "❌ WSL 未安装或未在 PATH 中" -ForegroundColor Red
    Write-Host "💡 请安装 WSL2: https://docs.microsoft.com/en-us/windows/wsl/install" -ForegroundColor Yellow
    exit 1
}

# 检查指定的发行版是否存在
$wslList = wsl --list --quiet
if ($wslList -notcontains $Distribution) {
    Write-Host "❌ WSL 发行版 '$Distribution' 未找到" -ForegroundColor Red
    Write-Host "📋 可用发行版:" -ForegroundColor Yellow
    wsl --list --verbose
    exit 1
}

Write-Host "✅ 使用 WSL 发行版: $Distribution" -ForegroundColor Green

# 获取 Windows 路径对应的 WSL 路径
$WindowsPath = Get-Location
$WSLPath = $WindowsPath.Path -replace '\\', '/' -replace '^([A-Z]):', '/mnt/$($1.ToLower())'

Write-Host "📁 项目路径: $WSLPath" -ForegroundColor Gray

# 构建编译命令
$BuildType = if ($Release) { "--release" } else { "" }
$BuildMode = if ($Release) { "Release" } else { "Debug" }

Write-Host "🔨 编译模式: $BuildMode" -ForegroundColor Yellow

# 如果需要安装依赖
if ($Install) {
    Write-Host "📦 安装 Rust 和依赖..." -ForegroundColor Yellow
    
    $InstallScript = @"
set -e
cd '$WSLPath'

# 更新包管理器
sudo apt update

# 安装必要的依赖
sudo apt install -y build-essential libssl-dev pkg-config curl

# 检查 Rust 是否已安装
if ! command -v rustc &> /dev/null; then
    echo "📥 安装 Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source ~/.cargo/env
else
    echo "✅ Rust 已安装: \$(rustc --version)"
fi

# 确保 cargo 在 PATH 中
source ~/.cargo/env

echo "🎯 Rust 环境准备完成"
"@

    wsl -d $Distribution bash -c $InstallScript
    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ 依赖安装失败" -ForegroundColor Red
        exit 1
    }
}

# 编译脚本
$CompileScript = @"
set -e
cd '$WSLPath'

# 确保 Rust 环境可用
source ~/.cargo/env

echo "🔍 检查 Rust 环境..."
rustc --version
cargo --version

echo "🔨 开始编译..."
cargo build $BuildType

if [ \$? -eq 0 ]; then
    echo "✅ 编译成功!"
    
    # 显示编译结果
    BUILD_DIR="target/$(if [ '$Release' = 'True' ]; then echo 'release'; else echo 'debug'; fi)"
    BINARY_PATH="\$BUILD_DIR/deepseek_proxy"
    
    if [ -f "\$BINARY_PATH" ]; then
        FILE_SIZE=\$(du -h "\$BINARY_PATH" | cut -f1)
        echo "📊 二进制文件: \$BINARY_PATH"
        echo "📏 文件大小: \$FILE_SIZE"
        
        # 检查依赖
        echo "🔗 依赖检查:"
        ldd "\$BINARY_PATH" | head -5
        
        # 设置执行权限
        chmod +x "\$BINARY_PATH"
        echo "✅ 已设置执行权限"
    fi
else
    echo "❌ 编译失败"
    exit 1
fi
"@

Write-Host "🚀 开始 WSL 编译..." -ForegroundColor Green
$StartTime = Get-Date

# 执行编译
wsl -d $Distribution bash -c $CompileScript

if ($LASTEXITCODE -eq 0) {
    $EndTime = Get-Date
    $Duration = $EndTime - $StartTime
    
    Write-Host "🎉 编译完成!" -ForegroundColor Green
    Write-Host "⏱️  耗时: $($Duration.TotalSeconds.ToString('F1'))秒" -ForegroundColor Gray
    
    # 显示文件位置
    $OutputDir = if ($Release) { "release" } else { "debug" }
    $BinaryPath = "target\$OutputDir\deepseek_proxy"
    
    if (Test-Path $BinaryPath) {
        $Size = (Get-Item $BinaryPath).Length / 1MB
        Write-Host "📁 Windows 路径: $BinaryPath" -ForegroundColor Gray
        Write-Host "📏 文件大小: $([math]::Round($Size, 2)) MB" -ForegroundColor Gray
    }
    
    Write-Host ""
    Write-Host "🚀 运行建议:" -ForegroundColor Yellow
    Write-Host "   # 在 WSL 中运行:" -ForegroundColor Gray
    Write-Host "   wsl -d $Distribution -- '$WSLPath/target/$OutputDir/deepseek_proxy'" -ForegroundColor Gray
    Write-Host ""
    Write-Host "   # 复制到 Linux 服务器:" -ForegroundColor Gray
    Write-Host "   scp $BinaryPath user@server:/opt/deepseek-proxy/" -ForegroundColor Gray
    
} else {
    Write-Host "❌ 编译失败" -ForegroundColor Red
    Write-Host "💡 常见解决方案:" -ForegroundColor Yellow
    Write-Host "   1. 安装依赖: .\build-wsl.ps1 -Install" -ForegroundColor Gray
    Write-Host "   2. 检查 WSL 网络连接" -ForegroundColor Gray
    Write-Host "   3. 更新 WSL: wsl --update" -ForegroundColor Gray
    exit 1
}