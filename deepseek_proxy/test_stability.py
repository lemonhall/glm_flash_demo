#!/usr/bin/env python3
"""DeepSeek 代理稳定性/压力测试脚本

场景:
 1. 批量创建并激活 N 个测试用户 (可通过环境变量控制是否跳过创建)
 2. 所有用户并发登录获取 token
 3. Round 1: 每个用户并发发送一次简短 chat 请求 ("说一个数字")
 4. 等待 REST_INTERVAL 秒
 5. Round 2: 再次发送一次并发请求
 6. Burst: 狂暴模式随机挑选 token 连续并发发送 BURST_REQUESTS 次请求

统计输出:
  - 成功/失败计数与成功率
  - 错误分类: timeout / http_status(429,5xx,其他) / exception
  - 延迟指标: min / max / avg / median / p95 / p99
  - TPS (以完成请求数 / 总耗时计算) 对于 burst 阶段

使用方法 (PowerShell):
  $env:PROXY_URL = "http://localhost:8877"
  python .\deepseek_proxy\test_stability.py

可用环境变量:
  PROXY_URL           代理地址 (默认 http://localhost:8877)
  USER_COUNT          创建/登录用户数 (默认 100)
  BASE_USERNAME       用户名前缀 (默认 "st_user")
  USER_PASSWORD       用户统一密码 (默认 "pass123")
  QUOTA_TIER          创建用户的配额档次 (默认 "basic")
  REST_INTERVAL       两轮之间休眠秒数 (默认 60)
  BURST_REQUESTS      狂暴模式请求数 (默认 500)
  BURST_CONCURRENCY   Burst 模式最大并发 (默认 50)
  CREATE_USERS        是否创建缺失用户 (1/0, 默认 1)
  CLEANUP_USERS       测试后停用用户 (1/0, 默认 0)
  TIMEOUT             单请求超时秒数 (默认 20)
    PARALLEL_CREATE     并发创建用户 (默认 1=启用)
    PARALLEL_CREATE_WORKERS  并发创建线程数 (默认 30)
    PARALLEL_ACTIVATE_WORKERS 并发激活线程数 (默认 30)

注意: 管理接口无需认证 (假设仅限 localhost), 若未来改为需要 admin token, 需在脚本中补充。
"""

from __future__ import annotations

import os
import sys
import time
import math
import json
import random
from dataclasses import dataclass, field
from pathlib import Path
import shutil
from statistics import mean, median
from typing import List, Dict, Tuple, Any
from concurrent.futures import ThreadPoolExecutor, as_completed

import httpx

# 共享客户端减少握手开销
SHARED_CLIENT = httpx.Client(timeout=REQUEST_TIMEOUT)

# -------------------------- 配置 --------------------------

PROXY_URL = os.getenv("PROXY_URL", "http://localhost:8877")
ADMIN_BASE = f"{PROXY_URL}/admin"
LOGIN_ENDPOINT = f"{PROXY_URL}/auth/login"
CHAT_ENDPOINT = f"{PROXY_URL}/chat/completions"

USER_COUNT = int(os.getenv("USER_COUNT", "100"))
BASE_USERNAME = os.getenv("BASE_USERNAME", "st_user")
USER_PASSWORD = os.getenv("USER_PASSWORD", "pass123")
QUOTA_TIER = os.getenv("QUOTA_TIER", "basic")
REST_INTERVAL = int(os.getenv("REST_INTERVAL", "60"))
BURST_REQUESTS = int(os.getenv("BURST_REQUESTS", "500"))
BURST_CONCURRENCY = int(os.getenv("BURST_CONCURRENCY", "50"))
CREATE_USERS = os.getenv("CREATE_USERS", "1") == "1"
CLEANUP_USERS = os.getenv("CLEANUP_USERS", "0") == "1"
PHYSICAL_CLEAN = os.getenv("PHYSICAL_CLEAN", "1") == "1"  # 结束后物理文件清理
PRE_CLEAN = os.getenv("PRE_CLEAN", "1") == "1"  # 开始前按前缀物理清理
REQUEST_TIMEOUT = float(os.getenv("TIMEOUT", "20"))
TOKEN_REFRESH_GRACE = int(os.getenv("TOKEN_REFRESH_GRACE", "5"))  # 距离过期不足该秒数自动刷新
RETRY_429 = os.getenv("RETRY_429", "1") == "1"
MAX_429_RETRY = int(os.getenv("MAX_429_RETRY", "2"))
RAMP_BATCH_SIZE = int(os.getenv("RAMP_BATCH_SIZE", "25"))
RAMP_INTERVAL = float(os.getenv("RAMP_INTERVAL", "1.5"))
REPORT_JSON = os.getenv("REPORT_JSON")  # 若设置则输出 JSON 报告

RANDOM_SEED = int(os.getenv("RANDOM_SEED", str(int(time.time()))))
random.seed(RANDOM_SEED)

# -------------------------- 数据结构 --------------------------

@dataclass
class RequestResult:
    ok: bool
    status: int | None
    error: str | None
    elapsed: float
    phase: str
    content_snippet: str = ""
    retries: int = 0

@dataclass
class TokenInfo:
    token: str
    expires_at: float  # epoch seconds
    username: str
    issued_at: float

@dataclass
class Metrics:
    phase: str
    results: List[RequestResult] = field(default_factory=list)

    def latency_list(self) -> List[float]:
        return [r.elapsed for r in self.results if r.ok]

    def summary(self) -> Dict[str, Any]:
        total = len(self.results)
        successes = sum(1 for r in self.results if r.ok)
        failures = total - successes
        latency = self.latency_list()
        latency.sort()
        def pct(p: float) -> float:
            if not latency:
                return 0.0
            idx = min(len(latency) - 1, math.ceil(p * len(latency)) - 1)
            return latency[idx]
        fail_latencies = [r.elapsed for r in self.results if not r.ok]
        fail_avg = mean(fail_latencies) if fail_latencies else 0.0
        fail_p95 = 0.0
        if fail_latencies:
            fl_sorted = sorted(fail_latencies)
            fail_p95 = fl_sorted[min(len(fl_sorted)-1, math.ceil(0.95*len(fl_sorted))-1)]
        # 错误分类细分
        cat = {"auth":0, "rate_limit":0, "upstream":0, "timeout":0, "other":0}
        for r in self.results:
            if r.ok:
                continue
            if r.error == "timeout":
                cat["timeout"] += 1
            elif r.status == 401:
                cat["auth"] += 1
            elif r.status == 429:
                cat["rate_limit"] += 1
            elif r.status and (500 <= r.status < 600):
                cat["upstream"] += 1
            else:
                cat["other"] += 1
        return {
            "phase": self.phase,
            "total": total,
            "successes": successes,
            "failures": failures,
            "success_rate": (successes / total * 100) if total else 0.0,
            "latency_min": min(latency) if latency else 0.0,
            "latency_max": max(latency) if latency else 0.0,
            "latency_avg": mean(latency) if latency else 0.0,
            "latency_median": median(latency) if latency else 0.0,
            "latency_p95": pct(0.95),
            "latency_p99": pct(0.99),
            "errors": self.error_breakdown(),
            "fail_latency_avg": fail_avg,
            "fail_latency_p95": fail_p95,
            "error_categories": cat,
        }

    def error_breakdown(self) -> Dict[str, int]:
        buckets: Dict[str, int] = {}
        for r in self.results:
            if r.ok:
                continue
            key = r.error or (f"HTTP_{r.status}" if r.status else "unknown")
            buckets[key] = buckets.get(key, 0) + 1
        return buckets

# -------------------------- 工具函数 --------------------------

def print_section(title: str):
    print("\n" + "=" * 70)
    print(f"  {title}")
    print("=" * 70 + "\n")

def create_user(username: str, password: str, quota_tier: str, client: httpx.Client) -> str:
    """创建单个用户
    返回: 'new' 新建; 'ok' 已存在或激活成功; 'fail' 失败
    """
    try:
        resp = client.post(
            f"{ADMIN_BASE}/users",
            json={"username": username, "password": password, "quota_tier": quota_tier},
            timeout=REQUEST_TIMEOUT,
        )
        if resp.status_code == 201:
            client.post(
                f"{ADMIN_BASE}/users/{username}/active",
                json={"is_active": True},
                timeout=REQUEST_TIMEOUT,
            )
            return 'new'
        if resp.status_code == 200 or (resp.status_code == 500 and "已存在" in resp.text):
            client.post(
                f"{ADMIN_BASE}/users/{username}/active",
                json={"is_active": True},
                timeout=REQUEST_TIMEOUT,
            )
            return 'ok'
        return 'fail'
    except Exception:
        return 'fail'

def create_users(count: int) -> Tuple[List[str], List[str]]:
    print(f"创建用户 (count={count}, create={CREATE_USERS}, parallel={PARALLEL_CREATE}) ...")
    usernames = [f"{BASE_USERNAME}{i:03d}" for i in range(count)]
    created: List[str] = []
    if not CREATE_USERS:
        print("跳过创建用户，根据前缀假定已经存在并激活。")
        return usernames, created
    if PARALLEL_CREATE:
        # 第一阶段: 并发创建
        print(f"并发创建阶段 workers={PARALLEL_CREATE_WORKERS} ...")
        results: Dict[str, str] = {}
        def create_task(u: str):
            return u, create_user(u, USER_PASSWORD, QUOTA_TIER, SHARED_CLIENT)
        with ThreadPoolExecutor(max_workers=PARALLEL_CREATE_WORKERS) as executor:
            futures = [executor.submit(create_task, u) for u in usernames]
            for fut in as_completed(futures):
                u, status = fut.result()
                results[u] = status
        # 第二阶段: 对未激活成功的用户再次激活 (避免 race)
        to_activate = [u for u, s in results.items() if s in ('new','ok')]
        print(f"并发激活阶段 users={len(to_activate)} workers={PARALLEL_ACTIVATE_WORKERS} ...")
        def activate(u: str):
            try:
                SHARED_CLIENT.post(
                    f"{ADMIN_BASE}/users/{u}/active",
                    json={"is_active": True},
                    timeout=REQUEST_TIMEOUT,
                )
            except Exception:
                pass
        with ThreadPoolExecutor(max_workers=PARALLEL_ACTIVATE_WORKERS) as executor:
            futures = [executor.submit(activate, u) for u in to_activate]
            for _ in as_completed(futures):
                pass
        new_cnt = sum(1 for s in results.values() if s == 'new')
        ok_cnt = sum(1 for s in results.values() if s in ('new','ok'))
        created = [u for u, s in results.items() if s == 'new']
        print(f"用户创建/激活完成: ok={ok_cnt}/{count} (new={new_cnt})")
        return usernames, created
    # 串行回退
    new_cnt = 0
    ok_cnt = 0
    for u in usernames:
        r = create_user(u, USER_PASSWORD, QUOTA_TIER, SHARED_CLIENT)
        if r == 'new':
            new_cnt += 1
            ok_cnt += 1
            created.append(u)
        elif r == 'ok':
            ok_cnt += 1
    print(f"用户创建/激活完成: ok={ok_cnt}/{count} (new={new_cnt})")
    return usernames, created

def login_user(username: str, password: str) -> TokenInfo | None:
    for attempt in range(2):
        try:
            resp = SHARED_CLIENT.post(
                LOGIN_ENDPOINT,
                json={"username": username, "password": password},
                timeout=REQUEST_TIMEOUT,
            )
            if resp.status_code == 200:
                data = resp.json()
                token = data.get("token")
                expires_in = data.get("expires_in", 60)
                if token:
                    now = time.time()
                    return TokenInfo(token=token, expires_at=now + expires_in, username=username, issued_at=now)
                return None
            elif resp.status_code == 401:
                return None
        except Exception:
            if attempt == 1:
                return None
            time.sleep(0.2)
    return None

def login_users(usernames: List[str]) -> Dict[str, TokenInfo]:
    print(f"并发登录 {len(usernames)} 用户 ...")
    tokens: Dict[str, TokenInfo] = {}
    with ThreadPoolExecutor(max_workers=50) as executor:
    futures = {executor.submit(login_user, u, USER_PASSWORD): u for u in usernames}
        for fut in as_completed(futures):
            u = futures[fut]
            info = fut.result()
            if info:
                tokens[u] = info
    print(f"登录成功: {len(tokens)}/{len(usernames)}")
    if len(tokens) < len(usernames):
        print("⚠ 部分用户登录失败，后续请求只使用成功登录的用户。")
    return tokens

def ensure_tokens_valid(tokens: Dict[str, TokenInfo]):
    """刷新即将过期的 token"""
    need_refresh = [u for u, ti in tokens.items() if ti.expires_at - time.time() < TOKEN_REFRESH_GRACE]
    if not need_refresh:
        return
    print(f"刷新即将过期 token: {len(need_refresh)}")
    with ThreadPoolExecutor(max_workers=20) as executor:
        futures = {executor.submit(login_user, u, USER_PASSWORD): u for u in need_refresh}
        for fut in as_completed(futures):
            u = futures[fut]
            info = fut.result()
            if info:
                tokens[u] = info

def send_chat(user: str, token_info: TokenInfo, content: str, phase: str, tokens_map: Dict[str, TokenInfo]) -> RequestResult:
    start = time.time()
    attempt = 0
    last_error = None
    while True:
        headers = {"Authorization": f"Bearer {token_info.token}"}
        payload = {
            "model": "deepseek-chat",
            "messages": [{"role": "user", "content": content}],
            "stream": False,
        }
        try:
            resp = SHARED_CLIENT.post(CHAT_ENDPOINT, json=payload, headers=headers, timeout=REQUEST_TIMEOUT)
            elapsed = time.time() - start
            if resp.status_code == 200:
                snippet = ""
                try:
                    data = resp.json()
                    if "choices" in data:
                        msg = data["choices"][0].get("message", {}).get("content", "")
                        snippet = msg[:30]
                except Exception:
                    snippet = ""
                return RequestResult(True, 200, None, elapsed, phase, snippet, retries=attempt)
            # 401 处理: 刷新 token 重试一次
            if resp.status_code == 401 and attempt == 0:
                refreshed = login_user(user, USER_PASSWORD)
                if refreshed:
                    tokens_map[user] = refreshed
                    token_info = refreshed
                    attempt += 1
                    continue
                return RequestResult(False, 401, "HTTP_401", elapsed, phase, retries=attempt)
            # 429 限流: 退避重试
            if resp.status_code == 429 and RETRY_429 and attempt < MAX_429_RETRY:
                backoff = (0.15 * (2 ** attempt))
                time.sleep(backoff)
                attempt += 1
                continue
            return RequestResult(False, resp.status_code, f"HTTP_{resp.status_code}", elapsed, phase, retries=attempt)
        except httpx.TimeoutException:
            elapsed = time.time() - start
            return RequestResult(False, None, "timeout", elapsed, phase, retries=attempt)
        except Exception as e:
            elapsed = time.time() - start
            last_error = e.__class__.__name__
            return RequestResult(False, None, last_error, elapsed, phase, retries=attempt)

def run_round(tokens: Dict[str, TokenInfo], phase: str) -> Metrics:
    print(f"开始 {phase} 并发请求 (users={len(tokens)}) ...")
    ensure_tokens_valid(tokens)
    metrics = Metrics(phase)
    content = "说一个数字"
    start_all = time.time()
    usernames = list(tokens.keys())
    batch_size = max(1, min(RAMP_BATCH_SIZE, len(usernames)))
    idx = 0
    with ThreadPoolExecutor(max_workers=min(100, len(tokens))) as executor:
        futures = []
        while idx < len(usernames):
            batch = usernames[idx: idx + batch_size]
            for u in batch:
                futures.append(executor.submit(send_chat, u, tokens[u], content, phase, tokens))
            idx += batch_size
            if idx < len(usernames):
                time.sleep(RAMP_INTERVAL)
        for fut in as_completed(futures):
            metrics.results.append(fut.result())
    elapsed_all = time.time() - start_all
    print(f"{phase} 完成，总耗时 {elapsed_all:.2f}s")
    return metrics

def run_burst(tokens: Dict[str, TokenInfo], requests: int, concurrency: int) -> Metrics:
    print(f"开始 Burst 模式: {requests} 请求, 并发={concurrency} ...")
    metrics = Metrics("burst")
    ensure_tokens_valid(tokens)
    token_list = list(tokens.items())
    if not token_list:
        print("无可用 token，跳过 Burst。")
        return metrics
    start_all = time.time()
    def task(i: int):
        user, ti = random.choice(token_list)
        # 单请求前检查是否快过期
        if ti.expires_at - time.time() < TOKEN_REFRESH_GRACE:
            refreshed = login_user(user, USER_PASSWORD)
            if refreshed:
                tokens[user] = refreshed
                ti = refreshed
        return send_chat(user, ti, "说一个数字", "burst", tokens)
    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = [executor.submit(task, i) for i in range(requests)]
        for fut in as_completed(futures):
            metrics.results.append(fut.result())
    total_elapsed = time.time() - start_all
    succ = sum(1 for r in metrics.results if r.ok)
    print(f"Burst 完成: 成功 {succ}/{len(metrics.results)}, 总耗时 {total_elapsed:.2f}s, 简易TPS={succ/total_elapsed if total_elapsed>0 else 0:.2f}")
    return metrics

def print_metrics(metrics: Metrics):
    s = metrics.summary()
    print(f"阶段: {s['phase']}")
    print(f"  请求总数: {s['total']} 成功: {s['successes']} 失败: {s['failures']} 成功率: {s['success_rate']:.2f}%")
    print(f"  延迟(ms): min={s['latency_min']*1000:.1f} max={s['latency_max']*1000:.1f} avg={s['latency_avg']*1000:.1f} median={s['latency_median']*1000:.1f} p95={s['latency_p95']*1000:.1f} p99={s['latency_p99']*1000:.1f}")
    if s['errors']:
        print("  错误分布:")
        for k, v in sorted(s['errors'].items(), key=lambda x: -x[1]):
            print(f"    - {k}: {v}")
    else:
        print("  错误分布: 无")

def cleanup_users(usernames: List[str]):
    if not CLEANUP_USERS:
        return
    print("清理: 停用测试用户 ...")
    for u in usernames:
        try:
            httpx.post(
                f"{ADMIN_BASE}/users/{u}/active",
                json={"is_active": False},
                timeout=REQUEST_TIMEOUT,
            )
        except Exception:
            pass

def physical_cleanup(created_usernames: List[str] | None = None, extra_usernames: List[str] | None = None, prefix_only: bool = False):
    """物理清理用户/配额/日志
    prefix_only: 仅按前缀删除 (用于前置)
    仅删除以 BASE_USERNAME 开头或本次创建的用户名。
    """
    base_dir = Path(__file__).resolve().parent
    users_dir = base_dir / 'data' / 'users'
    quotas_dir = base_dir / 'data' / 'quotas'
    logs_users_dir = base_dir.parent / 'logs' / 'users'
    targets: set[str] = set()
    if prefix_only:
        if users_dir.exists():
            for f in users_dir.glob(f"{BASE_USERNAME}*.toml"):
                targets.add(f.stem)
    else:
        if created_usernames:
            targets.update(created_usernames)
        if extra_usernames:
            targets.update(extra_usernames)
    if not targets:
        print("(物理清理: 无目标)")
        return
    removed_users = removed_quotas = removed_logs = 0
    for uname in sorted(targets):
        if not (uname.startswith(BASE_USERNAME) or (created_usernames and uname in created_usernames)):
            continue
        user_file = users_dir / f"{uname}.toml"
        quota_file = quotas_dir / f"{uname}.json"
        log_dir = logs_users_dir / uname
        try:
            if user_file.exists():
                user_file.unlink(); removed_users += 1
        except Exception: pass
        try:
            if quota_file.exists():
                quota_file.unlink(); removed_quotas += 1
        except Exception: pass
        try:
            if log_dir.exists():
                shutil.rmtree(log_dir, ignore_errors=True); removed_logs += 1
        except Exception: pass
    print(f"物理清理完成: 用户文件={removed_users}, 配额文件={removed_quotas}, 日志目录={removed_logs}")

def main():
    print_section("DeepSeek 代理稳定性/压力测试")
    print(f"代理地址: {PROXY_URL}")
    print(f"用户数量: {USER_COUNT}")
    print(f"休眠间隔: {REST_INTERVAL}s")
    print(f"Burst 请求数: {BURST_REQUESTS}, 并发: {BURST_CONCURRENCY}")
    print(f"创建用户: {CREATE_USERS}, 清理用户(API): {CLEANUP_USERS}, 前置清理: {PRE_CLEAN}, 结束物理清理: {PHYSICAL_CLEAN}")
    print(f"随机种子: {RANDOM_SEED}\n")

    # 基础可达性检测
    try:
        httpx.get(f"{PROXY_URL}/auth/login", timeout=2.0)
    except Exception:
        print("❌ 错误: 代理服务未启动! 请先运行: .\\start.ps1")
        sys.exit(1)

    # 前置物理清理
    if PRE_CLEAN:
        print_section("前置物理清理")
        physical_cleanup(prefix_only=True)

    # 1. 创建用户
    usernames, created_usernames = create_users(USER_COUNT)

    # 2. 登录获取 token
    tokens = login_users(usernames)
    if not tokens:
        print("❌ 无任何用户登录成功，退出。")
        cleanup_users(usernames)
        sys.exit(1)

    # 3. Round 1
    m1 = run_round(tokens, "round1")
    print_metrics(m1)

    # 4. Rest
    print(f"休眠 {REST_INTERVAL}s 等待 ...")
    time.sleep(REST_INTERVAL)

    # 5. Round 2
    m2 = run_round(tokens, "round2")
    print_metrics(m2)

    # 6. Burst
    mb = run_burst(tokens, BURST_REQUESTS, BURST_CONCURRENCY)
    print_metrics(mb)

    # 汇总
    print_section("测试汇总")
    all_metrics = [m1, m2, mb]
    for m in all_metrics:
        print_metrics(m)

    if REPORT_JSON:
        try:
            report_data = {"phases": [m.summary() for m in all_metrics],
                           "config": {
                               "user_count": USER_COUNT,
                               "burst_requests": BURST_REQUESTS,
                               "burst_concurrency": BURST_CONCURRENCY,
                               "ramp_batch_size": RAMP_BATCH_SIZE,
                               "ramp_interval": RAMP_INTERVAL,
                               "retry_429": RETRY_429,
                               "max_429_retry": MAX_429_RETRY,
                               "token_refresh_grace": TOKEN_REFRESH_GRACE,
                           },
                           "timestamp": time.time()}
            with open(REPORT_JSON, "w", encoding="utf-8") as f:
                json.dump(report_data, f, ensure_ascii=False, indent=2)
            print(f"\n已写入报告 JSON: {REPORT_JSON}")
        except Exception as e:
            print(f"\n报告写入失败: {e}")

    # 退出码: 若 burst 或 round 有大量错误 (>20%) 认为失败
    failed_phases = []
    for m in all_metrics:
        s = m.summary()
        if s['total'] and s['failures'] / s['total'] > 0.20:
            failed_phases.append(s['phase'])
    if failed_phases:
        print(f"\n⚠ 测试完成，但以下阶段失败率过高: {failed_phases}")
        exit_code = 1
    else:
        print("\n🎉 测试完成，整体稳定性良好")
        exit_code = 0

    # 可选清理
    cleanup_users(usernames)
    if PHYSICAL_CLEAN:
        print_section("后置物理清理")
        physical_cleanup(created_usernames=created_usernames, extra_usernames=[u for u in usernames if u.startswith(BASE_USERNAME)], prefix_only=False)
    try:
        SHARED_CLIENT.close()
    except Exception:
        pass
    sys.exit(exit_code)

if __name__ == "__main__":
    main()
