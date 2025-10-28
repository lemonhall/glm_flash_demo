#!/usr/bin/env python3
"""直接测试 GLM API"""

import os
import httpx
import json

API_KEY = os.getenv("GLM_FLASH_API_KEY")
BASE_URL = "https://open.bigmodel.cn/api/paas/v4"

if not API_KEY:
    print("❌ 未设置 GLM_FLASH_API_KEY 环境变量")
    exit(1)

print(f"✓ API Key: {API_KEY[:10]}...")
print(f"✓ 请求地址: {BASE_URL}/chat/completions\n")

request_data = {
    "model": "glm-4.5-flash",
    "messages": [{"role": "user", "content": "用一句话介绍智谱AI"}],
    "stream": True,
}

print("📤 发送请求...")
print(f"请求数据: {json.dumps(request_data, ensure_ascii=False, indent=2)}\n")

try:
    with httpx.Client(timeout=30.0) as client:
        with client.stream(
            "POST",
            f"{BASE_URL}/chat/completions",
            json=request_data,
            headers={
                "Authorization": f"Bearer {API_KEY}",
                "Content-Type": "application/json",
            },
        ) as response:
            print(f"状态码: {response.status_code}")
            print(f"响应头: {dict(response.headers)}\n")
            
            if response.status_code != 200:
                print(f"❌ 错误: {response.text}")
                exit(1)
            
            print("📥 原始流数据:")
            print("-" * 60)
            
            line_count = 0
            for line in response.iter_lines():
                line_count += 1
                print(f"[Line {line_count}] {repr(line)}")
                
                if line.startswith("data: "):
                    data_str = line[6:]
                    if data_str.strip() == "[DONE]":
                        print("  → [DONE]")
                        break
                    
                    try:
                        chunk = json.loads(data_str)
                        if "choices" in chunk:
                            delta = chunk["choices"][0].get("delta", {})
                            if "content" in delta:
                                print(f"  → 内容: {delta['content']}")
                    except json.JSONDecodeError as e:
                        print(f"  → JSON 解析失败: {e}")
            
            print("-" * 60)
            print(f"\n✓ 接收完成 (共 {line_count} 行)")

except Exception as e:
    print(f"❌ 请求失败: {e}")
    import traceback
    traceback.print_exc()
