# 🔄 DeepSeek Proxy 交叉编译指南

## 📋 支持的目标平台

### Linux 平台
- `x86_64-unknown-linux-gnu` - Linux x64 (glibc)
- `x86_64-unknown-linux-musl` - Linux x64 (musl, 静态链接)
- `aarch64-unknown-linux-gnu` - Linux ARM64 (glibc)
- `aarch64-unknown-linux-musl` - Linux ARM64 (musl, 静态链接)

### 推荐目标
- **生产服务器**: `x86_64-unknown-linux-gnu`
- **Docker容器**: `x86_64-unknown-linux-musl` (静态链接，无依赖)
- **ARM服务器**: `aarch64-unknown-linux-gnu` (Apple Silicon, AWS Graviton)

---

## 🛠️ 安装交叉编译工具链

### 1. 安装目标平台
```bash
# Linux x64 (推荐)
rustup target add x86_64-unknown-linux-gnu

# Linux x64 静态链接 (Docker推荐)
rustup target add x86_64-unknown-linux-musl

# Linux ARM64 (Apple Silicon服务器)
rustup target add aarch64-unknown-linux-gnu
```

### 2. 安装链接器 (Windows)

#### 使用 Chocolatey (推荐)
```powershell
# 安装 mingw-w64 工具链
choco install mingw

# 或者安装完整的 LLVM
choco install llvm
```

#### 手动安装
1. 下载 [mingw-w64](https://www.mingw-w64.org/downloads/)
2. 添加到 PATH: `C:\mingw64\bin`

### 3. 配置 Cargo

创建 `.cargo/config.toml`:
```toml
[target.x86_64-unknown-linux-gnu]
linker = "x86_64-linux-gnu-gcc"

[target.x86_64-unknown-linux-musl]
linker = "rust-lld"

[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
```

---

## 🚀 编译命令

### Linux x64 (glibc)
```bash
# 标准编译
cargo build --target x86_64-unknown-linux-gnu --release

# 生成文件: target/x86_64-unknown-linux-gnu/release/deepseek_proxy
```

### Linux x64 (musl - 静态链接)
```bash
# 静态编译 (推荐Docker部署)
cargo build --target x86_64-unknown-linux-musl --release

# 优势: 无依赖，可以在任何Linux发行版运行
```

### Linux ARM64
```bash
# ARM64编译 (Apple Silicon/AWS Graviton)
cargo build --target aarch64-unknown-linux-gnu --release
```

---

## 📦 简化编译脚本

创建 `build-linux.ps1`:
```powershell
#!/usr/bin/env pwsh
param(
    [string]$Target = "x86_64-unknown-linux-gnu",
    [switch]$Release
)

Write-Host "🔄 交叉编译到 Linux: $Target"

# 检查目标是否安装
$installed = rustup target list --installed | Select-String $Target
if (-not $installed) {
    Write-Host "📥 安装目标: $Target"
    rustup target add $Target
}

# 编译
$BuildType = if ($Release) { "--release" } else { "" }
$OutputDir = if ($Release) { "release" } else { "debug" }

Write-Host "🔨 开始编译..."
cargo build --target $Target $BuildType

if ($LASTEXITCODE -eq 0) {
    $BinaryPath = "target/$Target/$OutputDir/deepseek_proxy"
    Write-Host "✅ 编译成功: $BinaryPath"
    
    # 显示文件信息
    if (Test-Path $BinaryPath) {
        $Size = (Get-Item $BinaryPath).Length / 1MB
        Write-Host "📊 文件大小: $([math]::Round($Size, 2)) MB"
    }
} else {
    Write-Host "❌ 编译失败"
    exit 1
}
```

### 使用脚本
```powershell
# 默认 x64 debug
.\build-linux.ps1

# x64 release
.\build-linux.ps1 -Release

# ARM64 release
.\build-linux.ps1 -Target aarch64-unknown-linux-gnu -Release

# 静态链接版本
.\build-linux.ps1 -Target x86_64-unknown-linux-musl -Release
```

---

## 🐳 Docker 部署优化

### Dockerfile (静态链接版本)
```dockerfile
# 使用 scratch 基础镜像 (最小化)
FROM scratch

# 复制静态链接的二进制文件
COPY target/x86_64-unknown-linux-musl/release/deepseek_proxy /deepseek_proxy

# 复制配置文件
COPY config.toml /config.toml

# 创建数据目录
VOLUME ["/data"]

# 暴露端口
EXPOSE 8877

# 启动命令
ENTRYPOINT ["/deepseek_proxy"]
```

### 极简镜像构建
```bash
# 1. 交叉编译静态版本
cargo build --target x86_64-unknown-linux-musl --release

# 2. 构建 Docker 镜像
docker build -t deepseek-proxy:latest .

# 3. 运行容器
docker run -d \
  -p 8877:8877 \
  -v ./config.toml:/config.toml \
  -v ./data:/data \
  -e OPENAI_API_KEY=your_key \
  deepseek-proxy:latest
```

**优势**:
- 镜像大小 < 20MB
- 无系统依赖
- 安全性高
- 启动极快

---

## 🔧 常见问题解决

### 1. 链接器错误
```
error: linker `x86_64-linux-gnu-gcc` not found
```

**解决方案**:
```bash
# 安装 mingw-w64
choco install mingw

# 或使用 LLVM
choco install llvm
```

### 2. OpenSSL 依赖问题
```
error: failed to run custom build command for `openssl-sys`
```

**解决方案**: 使用 musl 目标 (静态链接)
```bash
cargo build --target x86_64-unknown-linux-musl --release
```

### 3. 依赖库交叉编译失败

**解决方案**: 检查 `Cargo.toml` 中的依赖
```toml
[dependencies]
# 确保使用支持交叉编译的版本
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }
tokio-rustls = "0.25"  # 替代 native-tls
```

### 4. 性能对比
```
# 编译时间
x86_64-pc-windows-msvc:     2m 30s
x86_64-unknown-linux-gnu:   2m 45s
x86_64-unknown-linux-musl:  3m 15s  (静态链接耗时更长)

# 二进制大小
Windows:  25MB
Linux:    22MB
Musl:     28MB  (静态链接包含所有依赖)
```

---

## 📊 推荐部署策略

### 开发环境
```bash
# 快速编译测试
cargo build --target x86_64-unknown-linux-gnu
```

### 生产环境
```bash
# 优化编译
RUSTFLAGS="-C target-cpu=native" \
cargo build --target x86_64-unknown-linux-gnu --release
```

### 容器化部署
```bash
# 静态链接 + strip优化
cargo build --target x86_64-unknown-linux-musl --release
strip target/x86_64-unknown-linux-musl/release/deepseek_proxy
```

### CI/CD 流水线
```yaml
# GitHub Actions 示例
- name: Build Linux Binary
  run: |
    rustup target add x86_64-unknown-linux-musl
    cargo build --target x86_64-unknown-linux-musl --release
    
- name: Upload Artifact
  uses: actions/upload-artifact@v3
  with:
    name: deepseek-proxy-linux
    path: target/x86_64-unknown-linux-musl/release/deepseek_proxy
```

---

## ⚡ 性能优化技巧

### 1. 编译优化
```toml
# Cargo.toml
[profile.release]
lto = true              # 链接时优化
codegen-units = 1       # 单线程代码生成
panic = "abort"         # 减少二进制大小
strip = true            # 移除调试信息
```

### 2. 目标 CPU 优化
```bash
# 针对特定 CPU 优化
RUSTFLAGS="-C target-cpu=x86-64-v3" \
cargo build --target x86_64-unknown-linux-gnu --release
```

### 3. 链接器优化
```bash
# 使用 mold 快速链接器 (Linux)
RUSTFLAGS="-C link-arg=-fuse-ld=mold" \
cargo build --target x86_64-unknown-linux-gnu --release
```

---

*最后更新: 2025-10-30*  
*维护者: DeepSeek Proxy Team*