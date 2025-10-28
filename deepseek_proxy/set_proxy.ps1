# 设置代理 - 加速 Rust 下载和编译

$proxyHost = "127.0.0.1"
$proxyPort = "7897"
$proxyUrl = "http://${proxyHost}:${proxyPort}"

Write-Host "===================================================" -ForegroundColor Cyan
Write-Host "  设置代理配置" -ForegroundColor Cyan
Write-Host "===================================================" -ForegroundColor Cyan
Write-Host ""

# 设置当前会话的代理
$env:HTTP_PROXY = $proxyUrl
$env:HTTPS_PROXY = $proxyUrl

Write-Host "✓ 已设置当前会话代理:" -ForegroundColor Green
Write-Host "  HTTP_PROXY  = $proxyUrl" -ForegroundColor Gray
Write-Host "  HTTPS_PROXY = $proxyUrl" -ForegroundColor Gray
Write-Host ""

# 可选：设置系统级代理（永久生效，需要管理员权限）
$setPermanent = Read-Host "是否设置为系统级永久代理? [y/N]"

if ($setPermanent -eq "y" -or $setPermanent -eq "Y") {
    try {
        # 设置用户级环境变量
        [System.Environment]::SetEnvironmentVariable("HTTP_PROXY", $proxyUrl, [System.EnvironmentVariableTarget]::User)
        [System.Environment]::SetEnvironmentVariable("HTTPS_PROXY", $proxyUrl, [System.EnvironmentVariableTarget]::User)
        
        Write-Host ""
        Write-Host "✓ 系统级代理已设置（永久生效）" -ForegroundColor Green
        Write-Host "  重启终端后生效" -ForegroundColor Yellow
    } catch {
        Write-Host ""
        Write-Host "❌ 设置系统级代理失败: $_" -ForegroundColor Red
        Write-Host "  当前会话的代理仍然有效" -ForegroundColor Yellow
    }
} else {
    Write-Host "仅当前会话有效（关闭终端后失效）" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "===================================================" -ForegroundColor Cyan
Write-Host "  代理配置完成" -ForegroundColor Cyan
Write-Host "===================================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "提示: 如需临时禁用代理，运行:" -ForegroundColor Gray
Write-Host "  Remove-Item Env:HTTP_PROXY" -ForegroundColor Cyan
Write-Host "  Remove-Item Env:HTTPS_PROXY" -ForegroundColor Cyan
Write-Host ""
