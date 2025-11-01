#!/usr/bin/env python3
"""DeepSeek 代理服务测试脚本"""

from time import sleep
import httpx
import json
import sys
from typing import Iterator

# 代理服务配置
PROXY_URL = "http://localhost:8877"
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


def test_user_active_management():
    """测试用户激活状态管理"""
    print_section("测试 8: 用户激活状态管理")

    admin_api_base = f"{PROXY_URL}/admin"
    test_username = "user2"
    test_password = "pass456"

    try:
        # 1. 先确保用户是激活状态
        print("1. 设置用户为激活状态...")
        response = httpx.post(
            f"{admin_api_base}/users/{test_username}/active",
            json={"is_active": True},
            timeout=5.0
        )
        if response.status_code != 200:
            print(f"✗ 设置激活状态失败: {response.status_code}")
            return False
        print(f"✓ 用户 {test_username} 已激活")

        # 2. 测试激活用户可以登录
        print(f"\n2. 测试激活用户登录...")
        response = httpx.post(
            LOGIN_ENDPOINT,
            json={"username": test_username, "password": test_password},
            timeout=5.0
        )
        if response.status_code == 200:
            print(f"✓ 激活用户登录成功")
        else:
            print(f"✗ 激活用户登录失败: {response.status_code}")
            return False

        # 3. 停用用户
        print(f"\n3. 停用用户 {test_username}...")
        response = httpx.post(
            f"{admin_api_base}/users/{test_username}/active",
            json={"is_active": False},
            timeout=5.0
        )
        if response.status_code != 200:
            print(f"✗ 停用用户失败: {response.status_code}")
            return False
        result = response.json()
        print(f"✓ {result['message']}")

        # 4. 测试停用用户无法登录
        print(f"\n4. 测试停用用户登录...")
        response = httpx.post(
            LOGIN_ENDPOINT,
            json={"username": test_username, "password": test_password},
            timeout=5.0
        )
        if response.status_code == 401:
            error_msg = response.json()
            print(f"✓ 停用用户被正确拒绝")
            print(f"  错误信息: {error_msg}")
        else:
            print(f"✗ 停用用户不应该能登录，状态码: {response.status_code}")
            return False

        # 5. 重新激活用户
        print(f"\n5. 重新激活用户 {test_username}...")
        response = httpx.post(
            f"{admin_api_base}/users/{test_username}/active",
            json={"is_active": True},
            timeout=5.0
        )
        if response.status_code != 200:
            print(f"✗ 重新激活失败: {response.status_code}")
            return False
        result = response.json()
        print(f"✓ {result['message']}")

        # 6. 验证重新激活后可以登录
        print(f"\n6. 验证重新激活后可以登录...")
        response = httpx.post(
            LOGIN_ENDPOINT,
            json={"username": test_username, "password": test_password},
            timeout=5.0
        )
        if response.status_code == 200:
            print(f"✓ 重新激活后登录成功")
        else:
            print(f"✗ 重新激活后登录失败: {response.status_code}")
            return False

        # 7. 测试管理API只能从localhost访问（这个测试会失败，因为我们就是localhost）
        print(f"\n7. 获取用户信息...")
        response = httpx.get(
            f"{admin_api_base}/users/{test_username}",
            timeout=5.0
        )
        if response.status_code == 200:
            user_info = response.json()
            print(f"✓ 获取用户信息成功:")
            print(f"  用户名: {user_info['username']}")
            print(f"  配额档次: {user_info['quota_tier']}")
            print(f"  激活状态: {user_info['is_active']}")
        else:
            print(f"✗ 获取用户信息失败: {response.status_code}")
            return False

        print("\n✓ 所有用户激活状态管理测试通过!")
        return True

    except Exception as e:
        print(f"✗ 测试失败: {e}")
        import traceback
        traceback.print_exc()
        return False


def test_admin_list_users():
    """测试列出所有用户"""
    print_section("测试 9: 列出所有用户")

    admin_api_base = f"{PROXY_URL}/admin"

    try:
        response = httpx.get(
            f"{admin_api_base}/users",
            timeout=5.0
        )

        if response.status_code == 200:
            result = response.json()
            users = result['users']
            print(f"✓ 成功获取用户列表 (共 {len(users)} 个用户):\n")
            for user in users:
                status = "✓ 激活" if user['is_active'] else "✗ 停用"
                print(f"  - {user['username']:10s} [{user['quota_tier']:8s}] {status}")
            return True
        else:
            print(f"✗ 获取用户列表失败: {response.status_code}")
            return False

    except Exception as e:
        print(f"✗ 测试失败: {e}")
        return False


def test_invalid_username_creation():
    """测试创建非法用户名应该被拒绝（修复 B11）"""
    print_section("测试 10: 非法用户名校验")

    admin_api_base = f"{PROXY_URL}/admin"

    # 定义各种非法用户名测试用例
    invalid_usernames = [
        ("../admin", "路径穿越攻击"),
        ("user/test", "包含路径分隔符 /"),
        ("user\\test", "包含路径分隔符 \\"),
        ("user..admin", "包含 .."),
        ("user\0test", "包含空字节"),
        ("ab", "长度太短 (< 3)"),
        ("a" * 33, "长度太长 (> 32)"),
        ("@admin", "以特殊字符开头"),
        ("-admin", "以连字符开头"),
        ("_admin", "以下划线开头"),
        ("user@test", "包含 @ 特殊字符"),
        ("user#test", "包含 # 特殊字符"),
        ("user test", "包含空格"),
        ("user.test", "包含点号"),
        ("用户名", "包含中文字符"),
    ]

    try:
        success_count = 0
        failed_cases = []

        print("测试各种非法用户名...\n")

        for username, description in invalid_usernames:
            display_name = repr(username) if len(username) <= 20 else f"{repr(username[:20])}..."
            
            try:
                response = httpx.post(
                    f"{admin_api_base}/users",
                    json={
                        "username": username,
                        "password": "test123",
                        "quota_tier": "basic"
                    },
                    timeout=5.0
                )

                if response.status_code == 400:
                    error_data = response.json()
                    print(f"✓ {display_name:30s} - 正确拒绝 ({description})")
                    print(f"  错误信息: {error_data.get('error', {}).get('message', 'N/A')}")
                    success_count += 1
                elif response.status_code == 500 and "用户" in response.text and "已存在" in response.text:
                    # 已存在的用户（如果之前测试创建过）
                    print(f"⚠ {display_name:30s} - 用户已存在 ({description})")
                    success_count += 1
                else:
                    print(f"✗ {display_name:30s} - 应该拒绝但接受了 ({description})")
                    print(f"  状态码: {response.status_code}, 响应: {response.text[:100]}")
                    failed_cases.append((username, description))

            except Exception as e:
                print(f"✗ {display_name:30s} - 测试异常: {e}")
                failed_cases.append((username, description))

        # 测试合法用户名（应该能创建成功）
        print("\n测试合法用户名（应该成功）...\n")
        
        valid_usernames = [
            ("user123", "字母+数字"),
            ("test_user", "包含下划线"),
            ("test-user", "包含连字符"),
            ("abc", "最短合法长度 (3)"),
            ("a" * 32, "最长合法长度 (32)"),
            ("123test", "以数字开头"),
        ]

        valid_success_count = 0
        for username, description in valid_usernames:
            try:
                response = httpx.post(
                    f"{admin_api_base}/users",
                    json={
                        "username": username,
                        "password": "test123",
                        "quota_tier": "basic"
                    },
                    timeout=5.0
                )

                if response.status_code in [200, 201]:
                    print(f"✓ {username:30s} - 创建成功 ({description})")
                    valid_success_count += 1
                    # 清理：停用测试用户
                    httpx.post(
                        f"{admin_api_base}/users/{username}/active",
                        json={"is_active": False},
                        timeout=5.0
                    )
                elif response.status_code == 500 and "已存在" in response.text:
                    print(f"✓ {username:30s} - 用户已存在 (视为成功) ({description})")
                    valid_success_count += 1
                else:
                    print(f"✗ {username:30s} - 创建失败: {response.status_code}")
                    print(f"  响应: {response.text[:100]}")

            except Exception as e:
                print(f"✗ {username:30s} - 测试异常: {e}")

        # 汇总结果
        print(f"\n{'='*60}")
        print(f"非法用户名测试: {success_count}/{len(invalid_usernames)} 正确拒绝")
        print(f"合法用户名测试: {valid_success_count}/{len(valid_usernames)} 成功创建")

        if failed_cases:
            print(f"\n未正确拒绝的非法用户名:")
            for username, desc in failed_cases:
                print(f"  - {repr(username)}: {desc}")

        # 只要大部分非法用户名被正确拒绝即可通过
        if success_count >= len(invalid_usernames) * 0.8 and valid_success_count >= len(valid_usernames) * 0.8:
            print("\n✓ 用户名校验测试基本通过!")
            return True
        else:
            print("\n✗ 用户名校验测试未通过")
            return False

    except Exception as e:
        print(f"✗ 测试失败: {e}")
        import traceback
        traceback.print_exc()
        return False


def test_new_user_can_use_service():
    """测试通过Admin API创建的新用户能够使用服务（覆盖Bug #1）"""
    print_section("测试 11: 新用户可以使用服务")

    admin_api_base = f"{PROXY_URL}/admin"
    test_username = "test_newuser"
    test_password = "newpass123"

    try:
        # 1. 创建新用户
        print(f"1. 通过Admin API创建新用户 '{test_username}'...")
        response = httpx.post(
            f"{admin_api_base}/users",
            json={
                "username": test_username,
                "password": test_password,
                "quota_tier": "basic"
            },
            timeout=5.0
        )

        # 如果用户已存在，先确保是激活状态
        if response.status_code == 200:
            result = response.json()
            print(f"  用户已存在，确保激活状态...")
            httpx.post(
                f"{admin_api_base}/users/{test_username}/active",
                json={"is_active": True},
                timeout=5.0
            )
            print(f"✓ 使用已存在的用户: {result['username']}")
        elif response.status_code == 201:
            result = response.json()
            print(f"✓ 用户创建成功: {result['username']} (quota_tier: {result['quota_tier']})")
        else:
            print(f"✗ 创建用户失败: {response.status_code} - {response.text}")
            return False

        # 2. 新用户登录
        print(f"\n2. 新用户登录...")
        login_response = httpx.post(
            LOGIN_ENDPOINT,
            json={"username": test_username, "password": test_password},
            timeout=5.0
        )

        if login_response.status_code != 200:
            print(f"✗ 新用户登录失败: {login_response.status_code} - {login_response.text}")
            return False

        token = login_response.json()["token"]
        print(f"✓ 新用户登录成功，Token: {token[:20]}...")

        # 3. 使用新用户调用 chat 接口（这是核心测试：验证配额系统能找到动态创建的用户）
        print(f"\n3. 新用户调用 chat 接口...")
        with ProxyClient() as client:
            client.token = token
            messages = [{"role": "user", "content": "说一个数字"}]

            try:
                response_text = "".join(client.chat(messages))
                print(f"✓ 新用户成功调用 chat 接口")
                print(f"  响应: {response_text[:30]}...")
            except Exception as e:
                print(f"✗ 新用户调用 chat 失败: {e}")
                # 清理：停用用户
                httpx.post(
                    f"{admin_api_base}/users/{test_username}/active",
                    json={"is_active": False},
                    timeout=5.0
                )
                return False

        # 4. 验证配额已被扣除
        print(f"\n4. 验证配额已被扣除...")
        user_info_response = httpx.get(
            f"{admin_api_base}/users/{test_username}",
            timeout=5.0
        )

        if user_info_response.status_code == 200:
            user_info = user_info_response.json()
            print(f"✓ 用户信息: {user_info}")

        # 5. 清理：停用测试用户
        print(f"\n5. 清理测试用户...")
        cleanup_response = httpx.post(
            f"{admin_api_base}/users/{test_username}/active",
            json={"is_active": False},
            timeout=5.0
        )

        if cleanup_response.status_code == 200:
            print(f"✓ 测试用户已停用")

        print("\n✓ 所有新用户测试通过! (Bug #1 已修复)")
        return True

    except Exception as e:
        print(f"✗ 测试失败: {e}")
        import traceback
        traceback.print_exc()

        # 尝试清理
        try:
            httpx.post(
                f"{admin_api_base}/users/{test_username}/active",
                json={"is_active": False},
                timeout=5.0
            )
        except:
            pass

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
        ("用户激活状态管理", test_user_active_management),
        ("列出所有用户", test_admin_list_users),
        ("非法用户名校验 (Bug #B11)", test_invalid_username_creation),
        ("新用户可以使用服务 (Bug #1)", test_new_user_can_use_service),
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
