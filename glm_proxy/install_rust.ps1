# Rust 工具链安装脚本
# 适用于 Windows 系统

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "  GLM 代理服务 - Rust 环境安装向导" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

# 检查是否已安装 Rust
$rustInstalled = Get-Command rustc -ErrorAction SilentlyContinue

if ($rustInstalled) {
    Write-Host "✓ Rust 已安装" -ForegroundColor Green
    Write-Host "  版本: $(rustc --version)" -ForegroundColor Gray
    Write-Host ""
    
    # 检查 cargo
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Write-Host "✓ Cargo 已安装" -ForegroundColor Green
        Write-Host "  版本: $(cargo --version)" -ForegroundColor Gray
        Write-Host ""
        Write-Host "环境已就绪！可以直接运行: cargo run" -ForegroundColor Green
        exit 0
    }
}

Write-Host "❌ 未检测到 Rust 工具链" -ForegroundColor Yellow
Write-Host ""
Write-Host "准备安装 Rust..." -ForegroundColor Cyan
Write-Host ""
Write-Host "安装选项:" -ForegroundColor Yellow
Write-Host "  1. 自动安装 (推荐) - 使用官方 rustup 安装器" -ForegroundColor White
Write-Host "  2. 手动安装 - 打开官方网站手动下载" -ForegroundColor White
Write-Host "  3. 取消安装" -ForegroundColor White
Write-Host ""

$choice = Read-Host "请选择 [1/2/3]"

switch ($choice) {
    "1" {
        Write-Host ""
        Write-Host "正在下载 rustup-init.exe ..." -ForegroundColor Cyan
        
        # 下载 rustup 安装器
        $rustupUrl = "https://win.rustup.rs/x86_64"
        $rustupPath = "$env:TEMP\rustup-init.exe"
        
        try {
            Invoke-WebRequest -Uri $rustupUrl -OutFile $rustupPath
            Write-Host "✓ 下载完成" -ForegroundColor Green
            Write-Host ""
            Write-Host "启动安装程序..." -ForegroundColor Cyan
            Write-Host "安装过程中请选择默认选项 (直接按回车)" -ForegroundColor Yellow
            Write-Host ""
            
            # 运行安装器
            Start-Process -FilePath $rustupPath -Wait -NoNewWindow
            
            Write-Host ""
            Write-Host "==================================================" -ForegroundColor Green
            Write-Host "  安装完成！" -ForegroundColor Green
            Write-Host "==================================================" -ForegroundColor Green
            Write-Host ""
            Write-Host "⚠️  重要提示:" -ForegroundColor Yellow
            Write-Host "  1. 关闭当前终端" -ForegroundColor White
            Write-Host "  2. 重新打开 PowerShell" -ForegroundColor White
            Write-Host "  3. 运行: cargo --version  (验证安装)" -ForegroundColor White
            Write-Host "  4. 运行: cargo run  (启动服务)" -ForegroundColor White
            Write-Host ""
            
        } catch {
            Write-Host "❌ 下载失败: $_" -ForegroundColor Red
            Write-Host ""
            Write-Host "请手动访问: https://rustup.rs/" -ForegroundColor Yellow
            Start-Process "https://rustup.rs/"
        }
    }
    
    "2" {
        Write-Host ""
        Write-Host "正在打开官方网站..." -ForegroundColor Cyan
        Start-Process "https://rustup.rs/"
        Write-Host ""
        Write-Host "请按照网站说明完成安装" -ForegroundColor Yellow
        Write-Host "安装后请关闭并重新打开终端" -ForegroundColor Yellow
    }
    
    "3" {
        Write-Host ""
        Write-Host "已取消安装" -ForegroundColor Gray
        exit 0
    }
    
    default {
        Write-Host ""
        Write-Host "❌ 无效选择" -ForegroundColor Red
        exit 1
    }
}
