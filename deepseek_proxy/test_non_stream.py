#!/usr/bin/env python3
"""测试非流式模式的速率限制"""
import httpx
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

PROXY_URL = "http://localhost:8080"
USERNAME = "admin"
PASSWORD = "admin123"

def send_request(idx: int, token: str):
    """发送单个非流式请求"""
    start = time.time()
    try:
        with httpx.Client(timeout=30.0) as client:
            response = client.post(
                f"{PROXY_URL}/chat/completions",
                json={
                    "model": "glm-4.5-flash",
                    "messages": [{"role": "user", "content": f"说数字 {idx}"}],
                    "stream": False,  # 非流式
                },
                headers={"Authorization": f"Bearer {token}"},
            )
            elapsed = time.time() - start
            if response.status_code == 200:
                content = response.json()["choices"][0]["message"]["content"]
                return idx, True, elapsed, content[:30]
            else:
                return idx, False, elapsed, f"状态码{response.status_code}"
    except Exception as e:
        elapsed = time.time() - start
        return idx, False, elapsed, str(e)[:50]

print("=" * 70)
print("  非流式模式 - 速率限制测试")
print("=" * 70)

# 登录
with httpx.Client() as client:
    login_resp = client.post(
        f"{PROXY_URL}/auth/login",
        json={"username": USERNAME, "password": PASSWORD}
    )
    if login_resp.status_code != 200:
        print(f"登录失败: {login_resp.status_code}")
        exit(1)
    token = login_resp.json()["token"]
    print(f"✓ 已登录\n")

# 测试3个并发请求
print("发送3个并发请求...\n")
test_start = time.time()

with ThreadPoolExecutor(max_workers=3) as executor:
    futures = [executor.submit(send_request, i, token) for i in range(3)]
    
    results = []
    for future in as_completed(futures):
        idx, success, elapsed, info = future.result()
        results.append((idx, success, elapsed, info))
        status = "✓" if success else "✗"
        print(f"{status} 请求{idx}: {elapsed:.2f}秒 - {info}")

total_time = time.time() - test_start
results.sort(key=lambda x: x[0])

print(f"\n总耗时: {total_time:.2f}秒")
success_count = sum(1 for _, success, _, _ in results if success)
print(f"成功: {success_count}/3")

if success_count == 3:
    print("\n✅ 所有请求成功！")
else:
    print(f"\n⚠️ 有 {3 - success_count} 个请求失败")

print("=" * 70)
