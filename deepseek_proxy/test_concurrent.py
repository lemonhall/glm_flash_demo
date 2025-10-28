#!/usr/bin/env python3
"""测试并发请求 - 直接对比GLM API vs 代理"""

import httpx
import json
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
import os

PROXY_URL = "http://localhost:8080"
GLM_URL = "https://open.bigmodel.cn/api/paas/v4"
API_KEY = os.getenv("GLM_FLASH_API_KEY")
USERNAME = "admin"
PASSWORD = "admin123"

def test_direct_glm(idx: int):
    """直接测试GLM API"""
    start = time.time()
    try:
        with httpx.Client(timeout=30.0) as client:
            response = client.post(
                f"{GLM_URL}/chat/completions",
                json={
                    "model": "glm-4.5-flash",
                    "messages": [{"role": "user", "content": f"说数字{idx}"}],
                    "stream": False,
                },
                headers={"Authorization": f"Bearer {API_KEY}"},
            )
            elapsed = time.time() - start
            if response.status_code == 200:
                return idx, True, elapsed, response.json()["choices"][0]["message"]["content"][:20]
            else:
                return idx, False, elapsed, f"状态码{response.status_code}"
    except Exception as e:
        elapsed = time.time() - start
        return idx, False, elapsed, str(e)[:50]

def test_via_proxy(idx: int, token: str):
    """通过代理测试"""
    start = time.time()
    try:
        with httpx.Client(timeout=30.0) as client:
            response = client.post(
                f"{PROXY_URL}/chat/completions",
                json={
                    "model": "glm-4.5-flash",
                    "messages": [{"role": "user", "content": f"说数字{idx}"}],
                    "stream": False,
                },
                headers={"Authorization": f"Bearer {token}"},
            )
            elapsed = time.time() - start
            if response.status_code == 200:
                return idx, True, elapsed, response.json()["choices"][0]["message"]["content"][:20]
            else:
                return idx, False, elapsed, f"状态码{response.status_code}"
    except Exception as e:
        elapsed = time.time() - start
        return idx, False, elapsed, str(e)[:50]

print("=" * 60)
print("  并发测试对比: 直接 GLM API vs 代理服务")
print("=" * 60)

# 测试1: 直接访问GLM API
print("\n【测试1】直接并发访问 GLM API (3个请求)")
print("-" * 60)
with ThreadPoolExecutor(max_workers=3) as executor:
    futures = [executor.submit(test_direct_glm, i) for i in range(3)]
    for future in as_completed(futures):
        idx, success, elapsed, info = future.result()
        status = "✓" if success else "✗"
        print(f"{status} 请求{idx}: {elapsed:.2f}秒 - {info}")

time.sleep(3)  # 等待一下避免触发限流

# 测试2: 通过代理访问
print("\n【测试2】通过代理并发访问 (3个请求)")
print("-" * 60)

# 先登录获取token
with httpx.Client() as client:
    login_resp = client.post(
        f"{PROXY_URL}/auth/login",
        json={"username": USERNAME, "password": PASSWORD}
    )
    token = login_resp.json()["token"]
    print(f"✓ 已登录，Token: {token[:20]}...\n")

with ThreadPoolExecutor(max_workers=3) as executor:
    futures = [executor.submit(test_via_proxy, i, token) for i in range(3)]
    for future in as_completed(futures):
        idx, success, elapsed, info = future.result()
        status = "✓" if success else "✗"
        print(f"{status} 请求{idx}: {elapsed:.2f}秒 - {info}")

print("\n" + "=" * 60)
print("测试完成")
print("=" * 60)
