# 设置代理 - 加速下载
$env:HTTP_PROXY = "http://127.0.0.1:7897"
$env:HTTPS_PROXY = "http://127.0.0.1:7897"

rustup target add x86_64-unknown-linux-gnu