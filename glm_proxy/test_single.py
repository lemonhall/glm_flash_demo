#!/usr/bin/env python3
"""测试单次流式对话"""

import httpx
import json

PROXY_URL = "http://localhost:8080"
USERNAME = "admin"
PASSWORD = "admin123"

def main():
    with httpx.Client(timeout=30.0) as client:
        # 1. 登录
        print("1. 登录中...")
        login_resp = client.post(
            f"{PROXY_URL}/auth/login",
            json={"username": USERNAME, "password": PASSWORD}
        )
        token = login_resp.json()["token"]
        print(f"✓ Token: {token[:20]}...\n")
        
        # 2. 流式对话
        print("2. 发送消息: 用一句话介绍智谱AI\n")
        print("流式响应:")
        print("-" * 60)
        
        request_data = {
            "model": "glm-4.5-flash",
            "messages": [{"role": "user", "content": "用一句话介绍智谱AI"}],
            "stream": True,
        }
        
        full_response = ""
        with client.stream(
            "POST",
            f"{PROXY_URL}/chat/completions",
            json=request_data,
            headers={"Authorization": f"Bearer {token}"},
        ) as response:
            print(f"状态码: {response.status_code}")
            
            if response.status_code != 200:
                print(f"错误: {response.text}")
                return
            
            for line in response.iter_lines():
                if line.startswith("data: "):
                    data_str = line[6:]
                    if data_str.strip() == "[DONE]":
                        break
                    
                    try:
                        chunk = json.loads(data_str)
                        if "choices" in chunk:
                            delta = chunk["choices"][0].get("delta", {})
                            if "content" in delta:
                                content = delta["content"]
                                print(content, end="", flush=True)
                                full_response += content
                    except json.JSONDecodeError:
                        continue
        
        print("\n" + "-" * 60)
        print(f"\n✓ 完成 (共 {len(full_response)} 字符)")

if __name__ == "__main__":
    main()
