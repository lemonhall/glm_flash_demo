# 测试脚本 - 登录并测试聊天

# 1. 登录获取 token
$loginResponse = Invoke-RestMethod -Uri "http://localhost:8080/auth/login" `
    -Method POST `
    -ContentType "application/json" `
    -Body (@{
        username = "user1"
        password = "pass123"
    } | ConvertTo-Json)

$token = $loginResponse.token
Write-Host "✓ 登录成功，Token: $token" -ForegroundColor Green
Write-Host "✓ 有效期: $($loginResponse.expires_in) 秒`n" -ForegroundColor Green

# 2. 使用 token 调用聊天接口
Write-Host "正在调用聊天接口..." -ForegroundColor Yellow

$chatBody = @{
    model = "glm-4.5-flash"
    messages = @(
        @{
            role = "user"
            content = "用一句话介绍一下 Rust 语言"
        }
    )
    temperature = 0.95
    stream = $true
} | ConvertTo-Json

# 流式输出示例 (使用 curl 更好支持 SSE)
Write-Host "`n流式响应:" -ForegroundColor Cyan
curl.exe -X POST "http://localhost:8080/chat/completions" `
    -H "Authorization: Bearer $token" `
    -H "Content-Type: application/json" `
    -d $chatBody `
    --no-buffer

Write-Host "`n`n✓ 测试完成" -ForegroundColor Green
