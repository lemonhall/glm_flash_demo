# Rust 工具链安装脚本
# 适用于 Windows 系统

# 设置代理 - 加速下载
$env:HTTP_PROXY = "http://127.0.0.1:7897"
$env:HTTPS_PROXY = "http://127.0.0.1:7897"

Write-Host "🌐 已配置代理: 127.0.0.1:7897" -ForegroundColor Cyan
Write-Host ""

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
Write-Host "💾 磁盘空间说明:" -ForegroundColor Yellow
Write-Host "   Rust 工具链大约需要 3-5 GB 空间" -ForegroundColor Gray
Write-Host "   默认安装在 C 盘，但可以自定义安装路径" -ForegroundColor Gray
Write-Host ""
Write-Host "安装选项:" -ForegroundColor Yellow
Write-Host "  1. 安装到 D 盘 (推荐) - 节省 C 盘空间" -ForegroundColor White
Write-Host "  2. 安装到 C 盘 (默认) - 使用默认路径" -ForegroundColor White
Write-Host "  3. 自定义路径 - 手动指定安装位置" -ForegroundColor White
Write-Host "  4. 手动安装 - 打开官方网站" -ForegroundColor White
Write-Host "  5. 取消安装" -ForegroundColor White
Write-Host ""

$choice = Read-Host "请选择 [1/2/3/4/5]"

switch ($choice) {
    "1" {
        # 安装到 D 盘
        $rustupHome = "D:\Rust\rustup"
        $cargoHome = "D:\Rust\cargo"
        
        Write-Host ""
        Write-Host "📂 安装路径:" -ForegroundColor Cyan
        Write-Host "   RUSTUP_HOME: $rustupHome" -ForegroundColor Gray
        Write-Host "   CARGO_HOME:  $cargoHome" -ForegroundColor Gray
        Write-Host ""
        
        # 确认
        $confirm = Read-Host "确认安装到 D 盘? [Y/n]"
        if ($confirm -and $confirm -ne "Y" -and $confirm -ne "y") {
            Write-Host "已取消" -ForegroundColor Gray
            exit 0
        }
        
        Write-Host ""
        Write-Host "正在下载 rustup-init.exe ..." -ForegroundColor Cyan
        
        $rustupUrl = "https://win.rustup.rs/x86_64"
        $rustupPath = "$env:TEMP\rustup-init.exe"
        
        try {
            Invoke-WebRequest -Uri $rustupUrl -OutFile $rustupPath
            Write-Host "✓ 下载完成" -ForegroundColor Green
            Write-Host ""
            
            # 设置环境变量并安装
            $env:RUSTUP_HOME = $rustupHome
            $env:CARGO_HOME = $cargoHome
            
            Write-Host "启动安装程序 (安装到 D 盘)..." -ForegroundColor Cyan
            Write-Host "安装过程中直接按回车选择默认选项" -ForegroundColor Yellow
            Write-Host ""
            
            # 运行安装器
            & $rustupPath -y --default-toolchain stable
            
            Write-Host ""
            Write-Host "==================================================" -ForegroundColor Green
            Write-Host "  安装完成！" -ForegroundColor Green
            Write-Host "==================================================" -ForegroundColor Green
            Write-Host ""
            Write-Host "✓ Rust 已安装到 D 盘" -ForegroundColor Green
            Write-Host ""
            Write-Host "⚠️  重要提示:" -ForegroundColor Yellow
            Write-Host "  1. 关闭当前终端" -ForegroundColor White
            Write-Host "  2. 重新打开 PowerShell" -ForegroundColor White
            Write-Host "  3. 运行: cargo --version  (验证安装)" -ForegroundColor White
            Write-Host "  4. 运行: cargo run  (启动服务)" -ForegroundColor White
            Write-Host ""
            Write-Host "📝 环境变量已自动配置:" -ForegroundColor Cyan
            Write-Host "   RUSTUP_HOME = $rustupHome" -ForegroundColor Gray
            Write-Host "   CARGO_HOME = $cargoHome" -ForegroundColor Gray
            Write-Host "   PATH 已添加: $cargoHome\bin" -ForegroundColor Gray
            Write-Host ""
            
        } catch {
            Write-Host "❌ 安装失败: $_" -ForegroundColor Red
            Write-Host ""
            Write-Host "请手动访问: https://rustup.rs/" -ForegroundColor Yellow
            Start-Process "https://rustup.rs/"
        }
    }
    
    "2" {
        # 安装到 C 盘 (默认)
        Write-Host ""
        Write-Host "正在下载 rustup-init.exe ..." -ForegroundColor Cyan
        
        $rustupUrl = "https://win.rustup.rs/x86_64"
        $rustupPath = "$env:TEMP\rustup-init.exe"
        
        try {
            Invoke-WebRequest -Uri $rustupUrl -OutFile $rustupPath
            Write-Host "✓ 下载完成" -ForegroundColor Green
            Write-Host ""
            Write-Host "启动安装程序 (默认 C 盘)..." -ForegroundColor Cyan
            Write-Host "安装过程中直接按回车选择默认选项" -ForegroundColor Yellow
            Write-Host ""
            
            # 运行安装器 (使用默认路径)
            & $rustupPath -y --default-toolchain stable
            
            Write-Host ""
            Write-Host "==================================================" -ForegroundColor Green
            Write-Host "  安装完成！" -ForegroundColor Green
            Write-Host "==================================================" -ForegroundColor Green
            Write-Host ""
            Write-Host "⚠️  重要提示:" -ForegroundColor Yellow
            Write-Host "  1. 关闭当前终端" -ForegroundColor White
            Write-Host "  2. 重新打开 PowerShell" -ForegroundColor White
            Write-Host "  3. 运行: cargo --version  (验证安装)" -ForegroundColor White
            Write-Host ""
            
        } catch {
            Write-Host "❌ 安装失败: $_" -ForegroundColor Red
            Write-Host ""
            Write-Host "请手动访问: https://rustup.rs/" -ForegroundColor Yellow
            Start-Process "https://rustup.rs/"
        }
    }
    
    "3" {
        # 自定义路径
        Write-Host ""
        $customPath = Read-Host "请输入安装根目录 (例如: E:\MyTools)"
        
        if (-not $customPath) {
            Write-Host "❌ 路径不能为空" -ForegroundColor Red
            exit 1
        }
        
        $rustupHome = "$customPath\Rust\rustup"
        $cargoHome = "$customPath\Rust\cargo"
        
        Write-Host ""
        Write-Host "📂 安装路径:" -ForegroundColor Cyan
        Write-Host "   RUSTUP_HOME: $rustupHome" -ForegroundColor Gray
        Write-Host "   CARGO_HOME:  $cargoHome" -ForegroundColor Gray
        Write-Host ""
        
        $confirm = Read-Host "确认安装到此路径? [Y/n]"
        if ($confirm -and $confirm -ne "Y" -and $confirm -ne "y") {
            Write-Host "已取消" -ForegroundColor Gray
            exit 0
        }
        
        Write-Host ""
        Write-Host "正在下载 rustup-init.exe ..." -ForegroundColor Cyan
        
        $rustupUrl = "https://win.rustup.rs/x86_64"
        $rustupPath = "$env:TEMP\rustup-init.exe"
        
        try {
            Invoke-WebRequest -Uri $rustupUrl -OutFile $rustupPath
            Write-Host "✓ 下载完成" -ForegroundColor Green
            Write-Host ""
            
            $env:RUSTUP_HOME = $rustupHome
            $env:CARGO_HOME = $cargoHome
            
            Write-Host "启动安装程序..." -ForegroundColor Cyan
            Write-Host ""
            
            & $rustupPath -y --default-toolchain stable
            
            Write-Host ""
            Write-Host "==================================================" -ForegroundColor Green
            Write-Host "  安装完成！" -ForegroundColor Green
            Write-Host "==================================================" -ForegroundColor Green
            Write-Host ""
            Write-Host "⚠️  重要提示:" -ForegroundColor Yellow
            Write-Host "  1. 关闭当前终端" -ForegroundColor White
            Write-Host "  2. 重新打开 PowerShell" -ForegroundColor White
            Write-Host "  3. 运行: cargo --version" -ForegroundColor White
            Write-Host ""
            
        } catch {
            Write-Host "❌ 安装失败: $_" -ForegroundColor Red
            exit 1
        }
    }
    
    "4" {
        Write-Host ""
        Write-Host "正在打开官方网站..." -ForegroundColor Cyan
        Start-Process "https://rustup.rs/"
        Write-Host ""
        Write-Host "请按照网站说明完成安装" -ForegroundColor Yellow
        Write-Host "安装后请关闭并重新打开终端" -ForegroundColor Yellow
    }
    
    "5" {
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
