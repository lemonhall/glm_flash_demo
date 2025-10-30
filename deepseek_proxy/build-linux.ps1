#!/usr/bin/env pwsh
param(
    [string]$Target = "x86_64-unknown-linux-gnu",
    [switch]$Release,
    [switch]$Musl
)

# 如果指定了 musl，切换到 musl 目标
if ($Musl) {
    $Target = "x86_64-unknown-linux-musl"
}

Write-Host "🔄 交叉编译到 Linux: $Target" -ForegroundColor Cyan

# 检查目标是否安装
Write-Host "📋 检查目标平台..." -ForegroundColor Yellow
$installed = rustup target list --installed | Select-String $Target
if (-not $installed) {
    Write-Host "📥 安装目标: $Target" -ForegroundColor Green
    rustup target add $Target
    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ 目标安装失败" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "✅ 目标已安装: $Target" -ForegroundColor Green
}

# 设置编译参数
$BuildType = if ($Release) { "--release" } else { "" }
$OutputDir = if ($Release) { "release" } else { "debug" }

# 编译优化环境变量
if ($Release) {
    $env:RUSTFLAGS = "-C target-cpu=native -C strip=symbols"
}

Write-Host "🔨 开始编译..." -ForegroundColor Yellow
Write-Host "   目标: $Target" -ForegroundColor Gray
Write-Host "   模式: $(if ($Release) { 'Release' } else { 'Debug' })" -ForegroundColor Gray

# 执行编译
$StartTime = Get-Date
cargo build --target $Target $BuildType

if ($LASTEXITCODE -eq 0) {
    $EndTime = Get-Date
    $Duration = $EndTime - $StartTime
    
    $BinaryPath = "target/$Target/$OutputDir/deepseek_proxy"
    Write-Host "✅ 编译成功!" -ForegroundColor Green
    Write-Host "   耗时: $($Duration.TotalSeconds.ToString('F1'))秒" -ForegroundColor Gray
    Write-Host "   路径: $BinaryPath" -ForegroundColor Gray
    
    # 显示文件信息
    if (Test-Path $BinaryPath) {
        $Size = (Get-Item $BinaryPath).Length / 1MB
        Write-Host "   大小: $([math]::Round($Size, 2)) MB" -ForegroundColor Gray
        
        # 如果是 release 版本，显示优化信息
        if ($Release) {
            Write-Host "🎯 Release 优化已启用" -ForegroundColor Magenta
            if ($Target -like "*musl*") {
                Write-Host "🐳 静态链接版本，可用于 Docker 部署" -ForegroundColor Cyan
            }
        }
    }
    
    Write-Host ""
    Write-Host "🚀 部署建议:" -ForegroundColor Yellow
    Write-Host "   scp $BinaryPath user@server:/opt/deepseek-proxy/" -ForegroundColor Gray
    Write-Host "   ssh user@server 'chmod +x /opt/deepseek-proxy/deepseek_proxy'" -ForegroundColor Gray
    
} else {
    Write-Host "❌ 编译失败" -ForegroundColor Red
    Write-Host "💡 常见解决方案:" -ForegroundColor Yellow
    Write-Host "   1. 安装 mingw-w64: choco install mingw" -ForegroundColor Gray
    Write-Host "   2. 使用静态链接: .\build-linux.ps1 -Musl -Release" -ForegroundColor Gray
    Write-Host "   3. 检查网络连接和依赖" -ForegroundColor Gray
    exit 1
}

# 清理环境变量
if ($env:RUSTFLAGS) {
    Remove-Item Env:\RUSTFLAGS -ErrorAction SilentlyContinue
}