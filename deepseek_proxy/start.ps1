# 一键启动脚本 - 自动检查环境并运行服务
Stop-Process -Name "glm_proxy" -Force -ErrorAction SilentlyContinue

# 设置代理 - 加速 Cargo 下载依赖
$env:HTTP_PROXY = "http://127.0.0.1:7897"
$env:HTTPS_PROXY = "http://127.0.0.1:7897"

Write-Host "🌐 已配置代理: 127.0.0.1:7897" -ForegroundColor Cyan
Write-Host ""

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "  GLM 代理服务 - 一键启动" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

# 1. 检查 Rust 环境
Write-Host "[1/4] 检查 Rust 环境..." -ForegroundColor Yellow

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "❌ 未安装 Rust 工具链" -ForegroundColor Red
    Write-Host ""
    Write-Host "请先运行: .\install_rust.ps1" -ForegroundColor Yellow
    Write-Host "或访问: https://rustup.rs/" -ForegroundColor Yellow
    exit 1
}

Write-Host "✓ Rust 已安装: $(rustc --version)" -ForegroundColor Green
Write-Host ""

# 2. 检查 API Key
Write-Host "[2/4] 检查 API Key 配置..." -ForegroundColor Yellow

$apiKey = $env:GLM_FLASH_API_KEY
if (-not $apiKey) {
    Write-Host "⚠️  未设置 GLM_FLASH_API_KEY 环境变量" -ForegroundColor Yellow
    Write-Host ""
    $apiKey = Read-Host "请输入你的 GLM API Key (或按回车跳过)"
    
    if ($apiKey) {
        $env:GLM_FLASH_API_KEY = $apiKey
        Write-Host "✓ 已临时设置 API Key (仅本次会话有效)" -ForegroundColor Green
    } else {
        Write-Host "⚠️  警告: 未设置 API Key，服务可能无法正常工作" -ForegroundColor Yellow
    }
} else {
    Write-Host "✓ API Key 已配置" -ForegroundColor Green
}
Write-Host ""

# 3. 检查配置文件
Write-Host "[3/4] 检查配置文件..." -ForegroundColor Yellow

if (-not (Test-Path "config.toml")) {
    Write-Host "❌ 找不到 config.toml" -ForegroundColor Red
    exit 1
}

Write-Host "✓ 配置文件存在" -ForegroundColor Green
Write-Host ""

# 4. 启动服务
Write-Host "[4/4] 启动服务..." -ForegroundColor Yellow
Write-Host ""
Write-Host "首次运行会自动下载编译依赖，可能需要 5-10 分钟" -ForegroundColor Cyan
Write-Host "请耐心等待..." -ForegroundColor Cyan
Write-Host ""
Write-Host "--------------------------------------------------" -ForegroundColor Gray

try {
    # 运行 cargo
    cargo run
} catch {
    Write-Host ""
    Write-Host "❌ 启动失败: $_" -ForegroundColor Red
    exit 1
}
