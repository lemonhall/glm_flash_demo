#!/usr/bin/env python3
"""DeepSeek 代理服务测试脚本"""

import httpx
import time
import sys

BASE_URL = "http://localhost:8080"

def login():
    """登录获取 Token"""
    response = httpx.post(
        f"{BASE_URL}/auth/login",
        json={"username": "admin", "password": "admin123"},
        timeout=10
    )
    if response.status_code == 200:
        token = response.json()["token"]
        print("✓ 已登录")
        return token
    else:
        print(f"✗ 登录失败: {response.status_code}")
        sys.exit(1)

def test_stream_chat(token: str):
    """测试流式对话"""
    print("\n" + "=" * 70)
    print("  DeepSeek 流式对话测试")
    print("=" * 70)
    
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }
    
    request_data = {
        "model": "deepseek-chat",
        "messages": [
            {"role": "user", "content": "你好，请用一句话介绍 DeepSeek"}
        ],
        "stream": True,
        "temperature": 0.7
    }
    
    print(f"\n📤 发送请求...")
    start_time = time.time()
    
    try:
        with httpx.stream(
            "POST",
            f"{BASE_URL}/chat/completions",
            headers=headers,
            json=request_data,
            timeout=60
        ) as response:
            print(f"✓ 状态码: {response.status_code}")
            
            if response.status_code == 200:
                print("\n📥 流式响应内容:")
                print("-" * 70)
                
                for line in response.iter_lines():
                    if line.strip():
                        print(line)
                
                elapsed = time.time() - start_time
                print("-" * 70)
                print(f"\n✓ 完成，耗时: {elapsed:.2f}秒")
            else:
                print(f"✗ 请求失败: {response.status_code}")
                print(response.text)
                
    except Exception as e:
        print(f"✗ 错误: {e}")

def main():
    # 登录
    token = login()
    
    # 测试流式对话
    test_stream_chat(token)

if __name__ == "__main__":
    main()
