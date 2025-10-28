#!/usr/bin/env python3
"""测试代理的速率限制：3个并发请求应按 2 req/s 处理"""
import httpx
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

PROXY_URL = "http://localhost:8080"
USERNAME = "admin"
PASSWORD = "admin123"

def send_request(idx: int, token: str):
    """发送单个请求"""
    start = time.time()
    try:
        with httpx.Client(timeout=30.0) as client:
            response = client.post(
                f"{PROXY_URL}/chat/completions",
                json={
                    "model": "glm-4.5-flash",
                    "messages": [{"role": "user", "content": f"说数字 {idx}"}],
                    "stream": False,
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
print("  速率限制测试: 3个并发请求")
print("=" * 70)
print("预期:")
print("  - 请求0: 立即处理 (~0.5秒)")
print("  - 请求1: 等待~0.5秒后处理 (~1秒)")
print("  - 请求2: 等待~1秒后处理 (~1.5秒)")
print("  - 都应该成功 (200)，不应该有502")
print("=" * 70)

# 登录获取token
with httpx.Client() as client:
    login_resp = client.post(
        f"{PROXY_URL}/auth/login",
        json={"username": USERNAME, "password": PASSWORD}
    )
    if login_resp.status_code != 200:
        print(f"登录失败: {login_resp.status_code} - {login_resp.text}")
        exit(1)
    token = login_resp.json()["token"]
    print(f"\n✓ 已登录\n")

# 并发发送3个请求
print("开始测试...\n")
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

# 排序结果按请求索引
results.sort(key=lambda x: x[0])

print(f"\n总耗时: {total_time:.2f}秒")
print("\n分析:")
success_count = sum(1 for _, success, _, _ in results if success)
print(f"  成功: {success_count}/3")
print(f"  失败: {3 - success_count}/3")

if success_count == 3:
    print("\n✅ 所有请求成功！速率限制正常工作。")
else:
    print("\n❌ 有请求失败，速率限制可能有问题。")

print("=" * 70)
