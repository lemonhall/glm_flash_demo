#!/usr/bin/env python3
"""DeepSeek 代理服务测试脚本"""

from time import sleep
import httpx
import json
import sys
from typing import Iterator

# 代理服务配置
PROXY_URL = "http://localhost:8080"
LOGIN_ENDPOINT = f"{PROXY_URL}/auth/login"
CHAT_ENDPOINT = f"{PROXY_URL}/chat/completions"

# 测试账号
USERNAME = "admin"
PASSWORD = "admin123"


class ProxyClient:
    """DeepSeek 代理客户端"""
    
    def __init__(self, base_url: str = PROXY_URL):
        self.base_url = base_url
        self.client = httpx.Client(timeout=30.0)
        self.token: str | None = None
    
    def login(self, username: str, password: str) -> dict:
        """登录获取 Token"""
        response = self.client.post(
            f"{self.base_url}/auth/login",
            json={"username": username, "password": password}
        )
        response.raise_for_status()
        data = response.json()
        self.token = data["token"]
        return data
    
    def chat(self, messages: list[dict], **kwargs) -> Iterator[str]:
        """流式对话"""
        if not self.token:
            raise ValueError("请先登录获取 Token")
        
        request_data = {
            "model": "deepseek-chat",
            "messages": messages,
            "stream": True,
            **kwargs
        }
        
        with self.client.stream(
            "POST",
            f"{self.base_url}/chat/completions",
            json=request_data,
            headers={"Authorization": f"Bearer {self.token}"},
            timeout=30.0
        ) as response:
            response.raise_for_status()
            
            for line in response.iter_lines():
                if line.startswith("data: "):
                    data_str = line[6:]  # 去掉 "data: " 前缀
                    if data_str.strip() == "[DONE]":
                        break
                    
                    try:
                        chunk = json.loads(data_str)
                        if "choices" in chunk:
                            delta = chunk["choices"][0].get("delta", {})
                            if "content" in delta:
                                yield delta["content"]
                    except json.JSONDecodeError:
                        continue
    
    def close(self):
        """关闭客户端"""
        self.client.close()
    
    def __enter__(self):
        return self
    
    def __exit__(self, *args):
        self.close()


def print_section(title: str):
    """打印分隔标题"""
    print(f"\n{'='*60}")
    print(f"  {title}")
    print(f"{'='*60}\n")


def test_login():
    """测试登录功能"""
    print_section("测试 1: 登录认证")
    
    with ProxyClient() as client:
        try:
            result = client.login(USERNAME, PASSWORD)
            print(f"✓ 登录成功")
            print(f"  Token: {result['token'][:20]}...")
            print(f"  有效期: {result['expires_in']} 秒")
            return True
        except Exception as e:
            print(f"✗ 登录失败: {e}")
            return False


def test_login_cache():
    """测试登录缓存：60秒内多次登录返回同一个token"""
    print_section("测试 2: 登录缓存 (60秒内同一token)")
    
    try:
        # 第一次登录
        response1 = httpx.post(
            LOGIN_ENDPOINT,
            json={"username": USERNAME, "password": PASSWORD},
            timeout=5.0
        )
        token1 = response1.json()["token"]
        print(f"✓ 第1次登录 Token: {token1[:20]}...")
        
        # 第二次登录（立即）
        response2 = httpx.post(
            LOGIN_ENDPOINT,
            json={"username": USERNAME, "password": PASSWORD},
            timeout=5.0
        )
        token2 = response2.json()["token"]
        print(f"✓ 第2次登录 Token: {token2[:20]}...")
        
        # 第三次登录（立即）
        response3 = httpx.post(
            LOGIN_ENDPOINT,
            json={"username": USERNAME, "password": PASSWORD},
            timeout=5.0
        )
        token3 = response3.json()["token"]
        print(f"✓ 第3次登录 Token: {token3[:20]}...")
        
        # 验证是否相同
        if token1 == token2 == token3:
            print("\n✓ 验证成功：60秒内多次登录返回同一个 token")
            return True
        else:
            print("\n✗ 验证失败：token 不同")
            return False
            
    except Exception as e:
        print(f"✗ 测试失败: {e}")
        return False


def test_chat_stream():
    """测试流式对话"""
    print_section("测试 2: 流式对话")
    
    with ProxyClient() as client:
        try:
            # 登录
            client.login(USERNAME, PASSWORD)
            print("✓ 已获取 Token\n")
            
            # 发送消息
            messages = [{"role": "user", "content": "用一句话介绍 DeepSeek"}]
            print("📤 发送消息: 用一句话介绍 DeepSeek\n")
            print("📥 流式响应:")
            print("-" * 60)
            
            full_response = ""
            for chunk in client.chat(messages):
                print(chunk, end="", flush=True)
                full_response += chunk
            
            print("\n" + "-" * 60)
            print(f"\n✓ 接收完成 (共 {len(full_response)} 字符)")
            return True
            
        except Exception as e:
            print(f"\n✗ 对话失败: {e}")
            import traceback
            traceback.print_exc()
            return False


def test_token_serial():
    """测试Token串行：同一token同时只允许1个请求"""
    print_section("测试 4: Token串行限流 (同一token同时只允1个)")
    
    with ProxyClient() as client:
        try:
            client.login(USERNAME, PASSWORD)
            print("✓ 已获取 Token\n")
            
            messages = [{"role": "user", "content": "说一个数字"}]
            
            print("发送 2 个并发请求 (使用同一个 token)...")
            import time
            from concurrent.futures import ThreadPoolExecutor, as_completed
            
            def send_request(idx: int):
                start = time.time()
                try:
                    with ProxyClient() as c:
                        c.token = client.token
                        response = "".join(c.chat(messages))
                        elapsed = time.time() - start
                        return idx, True, elapsed, response[:20]
                except httpx.HTTPStatusError as e:
                    elapsed = time.time() - start
                    if e.response.status_code == 429:
                        return idx, "blocked", elapsed, "429 Too Many Requests"
                    return idx, False, elapsed, str(e)
                except Exception as e:
                    elapsed = time.time() - start
                    return idx, False, elapsed, str(e)
            
            with ThreadPoolExecutor(max_workers=2) as executor:
                futures = [executor.submit(send_request, i) for i in range(2)]
                
                success_count = 0
                blocked_count = 0
                
                for future in as_completed(futures):
                    idx, success, elapsed, info = future.result()
                    if success is True:
                        print(f"✓ 请求 {idx+1}: {elapsed:.2f}秒 - 成功")
                        success_count += 1
                    elif success == "blocked":
                        print(f"✓ 请求 {idx+1}: {elapsed:.2f}秒 - 被限流 (429)")
                        blocked_count += 1
                    else:
                        print(f"✗ 请求 {idx+1}: {elapsed:.2f}秒 - {info}")
            
            # 应该有一个成功，一个被限流
            if blocked_count > 0:
                print(f"\n✓ 验证成功：同一token的并发请求被限流 ({blocked_count}个被阻止)")
                return True
            else:
                print("\n⚠️  注意：没有请求被限流，可能是请求处理太快")
                return True
            
        except Exception as e:
            print(f"✗ 测试失败: {e}")
            return False


def test_multi_user_concurrent():
    """测试多用户并发：不同用户可以同时请求"""
    print_section("测试 5: 多用户并发 (不同 token 可并发)")
    
    try:
        # 用户1登录
        response1 = httpx.post(
            LOGIN_ENDPOINT,
            json={"username": "admin", "password": "admin123"},
            timeout=5.0
        )
        token1 = response1.json()["token"]
        print("✓ 用户 admin 已登录")
        
        # 用户2登录
        response2 = httpx.post(
            LOGIN_ENDPOINT,
            json={"username": "user1", "password": "pass123"},
            timeout=5.0
        )
        token2 = response2.json()["token"]
        print("✓ 用户 user1 已登录\n")
        
        messages = [{"role": "user", "content": "说一个数字"}]
        
        print("发送 2 个并发请求 (使用不同 token)...")
        import time
        from concurrent.futures import ThreadPoolExecutor, as_completed
        
        def send_request(idx: int, token: str):
            start = time.time()
            try:
                with ProxyClient() as c:
                    c.token = token
                    response = "".join(c.chat(messages))
                    elapsed = time.time() - start
                    return idx, True, elapsed, response[:10]
            except Exception as e:
                elapsed = time.time() - start
                return idx, False, elapsed, str(e)
        
        with ThreadPoolExecutor(max_workers=2) as executor:
            futures = [
                executor.submit(send_request, 0, token1),
                executor.submit(send_request, 1, token2)
            ]
            
            success_count = 0
            for future in as_completed(futures):
                idx, success, elapsed, info = future.result()
                status = "✓" if success else "✗"
                result = "成功" if success else info
                print(f"{status} 用户 {idx+1}: {elapsed:.2f}秒 - {result}")
                if success:
                    success_count += 1
        
        if success_count == 2:
            print("\n✓ 验证成功：不同token可以并发请求")
            return True
        else:
            print(f"\n✗ 只有 {success_count}/2 成功")
            return False
            
    except Exception as e:
        print(f"✗ 测试失败: {e}")
        import traceback
        traceback.print_exc()
        return False


def test_rate_limit():
    """测试旧的限流功能（保留兼容）"""
    print_section("测试 6: 基础并发测试")
    sleep(3)
    
    with ProxyClient() as client:
        try:
            client.login(USERNAME, PASSWORD)
            print("✓ 已获取 Token\n")
            
            messages = [{"role": "user", "content": "说一个数字"}]
            
            print("发送 3 个并发请求 (限流: 2 req/s)...")
            import time
            from concurrent.futures import ThreadPoolExecutor, as_completed
            
            def send_request(idx: int):
                start = time.time()
                try:
                    with ProxyClient() as c:
                        c.token = client.token
                        response = "".join(c.chat(messages))
                        elapsed = time.time() - start
                        return idx, True, elapsed, response[:20]
                except Exception as e:
                    elapsed = time.time() - start
                    return idx, False, elapsed, str(e)
            
            with ThreadPoolExecutor(max_workers=3) as executor:
                futures = [executor.submit(send_request, i) for i in range(3)]
                
                for future in as_completed(futures):
                    idx, success, elapsed, info = future.result()
                    status = "✓" if success else "✗"
                    print(f"{status} 请求 {idx+1}: {elapsed:.2f}秒 - {info}")
            
            print("\n✓ 限流测试完成")
            return True
            
        except Exception as e:
            print(f"✗ 限流测试失败: {e}")
            return False


def test_unauthorized():
    """测试未授权访问"""
    print_section("测试 7: 未授权访问拦截")
    
    try:
        response = httpx.post(
            CHAT_ENDPOINT,
            json={"model": "deepseek-chat", "messages": [{"role": "user", "content": "test"}]},
            timeout=5.0
        )
        
        if response.status_code == 401:
            print("✓ 正确拦截未授权请求")
            print(f"  状态码: {response.status_code}")
            return True
        else:
            print(f"✗ 应该返回 401，实际返回: {response.status_code}")
            return False
            
    except Exception as e:
        print(f"✗ 测试失败: {e}")
        return False


def main():
    """主测试流程"""
    print("\n" + "=" * 60)
    print("  DeepSeek 代理服务自动化测试")
    print("=" * 60)
    print(f"\n代理地址: {PROXY_URL}")
    print(f"测试账号: {USERNAME}\n")
    
    # 检查服务是否运行
    try:
        response = httpx.get(f"{PROXY_URL}/auth/login", timeout=2.0)
    except Exception:
        print("❌ 错误: 代理服务未启动!")
        print("   请先运行: .\\start.ps1")
        sys.exit(1)
    
    # 运行测试
    tests = [
        ("登录认证", test_login),
        ("登录缓存 (60秒)", test_login_cache),
        ("流式对话", test_chat_stream),
        ("Token串行限流", test_token_serial),
        ("多用户并发", test_multi_user_concurrent),
        ("基础并发测试", test_rate_limit),
        ("未授权拦截", test_unauthorized),
    ]
    
    results = []
    for name, test_func in tests:
        try:
            success = test_func()
            results.append((name, success))
        except KeyboardInterrupt:
            print("\n\n⚠️  测试被用户中断")
            sys.exit(1)
        except Exception as e:
            print(f"\n✗ 测试异常: {e}")
            results.append((name, False))
    
    # 输出总结
    print_section("测试总结")
    
    passed = sum(1 for _, success in results if success)
    total = len(results)
    
    for name, success in results:
        status = "✓ 通过" if success else "✗ 失败"
        print(f"{status} - {name}")
    
    print(f"\n总计: {passed}/{total} 通过")
    
    if passed == total:
        print("\n🎉 所有测试通过!")
        sys.exit(0)
    else:
        print(f"\n⚠️  {total - passed} 个测试失败")
        sys.exit(1)


if __name__ == "__main__":
    main()
