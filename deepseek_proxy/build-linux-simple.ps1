#!/usr/bin/env pwsh
# 简化版 WSL 编译脚本

Write-Host "🐧 WSL2 快速编译 DeepSeek Proxy" -ForegroundColor Cyan

# 获取当前路径的 WSL 格式
$WSLPath = (Get-Location).Path -replace '\\', '/' -replace '^([A-Z]):', '/mnt/$($1.ToLower())'

Write-Host "📁 项目路径: $WSLPath" -ForegroundColor Gray
Write-Host "🔨 开始编译..." -ForegroundColor Yellow

# 一键编译命令
wsl bash -c "cd '$WSLPath' && source ~/.cargo/env && cargo build --release"

if ($LASTEXITCODE -eq 0) {
    Write-Host "✅ 编译成功!" -ForegroundColor Green
    
    # 检查文件
    if (Test-Path "target\release\deepseek_proxy") {
        $Size = (Get-Item "target\release\deepseek_proxy").Length / 1MB
        Write-Host "📊 二进制: target\release\deepseek_proxy ($([math]::Round($Size, 2)) MB)" -ForegroundColor Gray
    }
} else {
    Write-Host "❌ 编译失败，尝试完整版本:" -ForegroundColor Red
    Write-Host "   .\build-wsl.ps1 -Install -Release" -ForegroundColor Gray
}