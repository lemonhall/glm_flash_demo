#!/usr/bin/env python3
"""
DeepSeek 代理 - 管理面板（一体化 FastAPI 应用）
启动: python app.py
访问: http://127.0.0.1:8089
"""
from fastapi import FastAPI
from fastapi.responses import FileResponse, JSONResponse
from fastapi.staticfiles import StaticFiles
import httpx
import json
import re
from pathlib import Path
from datetime import datetime

app = FastAPI(title="DeepSeek Admin Panel")

# ============ 配置 ============
PROXY_URL = "http://localhost:8877"
METRICS_URL = f"{PROXY_URL}/metrics"
USERS_DIR = Path(__file__).parent.parent / "deepseek_proxy" / "data" / "users"
LOGS_DIR = Path(__file__).parent.parent / "deepseek_proxy" / "logs" / "users"

# ============ 前端 HTML ============
FRONTEND_HTML = """<!DOCTYPE html>
<html lang="zh-cn">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>DeepSeek 代理 - 管理面板</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
      background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
      min-height: 100vh;
      padding: 20px;
    }
    .container {
      max-width: 1200px;
      margin: 0 auto;
    }
    header {
      color: white;
      margin-bottom: 30px;
    }
    header h1 {
      font-size: 28px;
      margin-bottom: 5px;
    }
    header p {
      font-size: 14px;
      opacity: 0.9;
    }
    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
      gap: 20px;
      margin-bottom: 30px;
    }
    .card {
      background: white;
      border-radius: 8px;
      padding: 20px;
      box-shadow: 0 2px 10px rgba(0,0,0,0.1);
      transition: transform 0.2s;
    }
    .card:hover {
      transform: translateY(-2px);
      box-shadow: 0 4px 20px rgba(0,0,0,0.15);
    }
    .card-label {
      font-size: 12px;
      color: #999;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      margin-bottom: 10px;
    }
    .card-value {
      font-size: 32px;
      font-weight: bold;
      color: #333;
    }
    .card.alert .card-value { color: #e74c3c; }
    .card.warning .card-value { color: #f39c12; }
    .card.success .card-value { color: #27ae60; }
    .charts {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(500px, 1fr));
      gap: 20px;
    }
    .chart-container {
      background: white;
      border-radius: 8px;
      padding: 20px;
      box-shadow: 0 2px 10px rgba(0,0,0,0.1);
    }
    .chart-title {
      font-size: 16px;
      font-weight: bold;
      margin-bottom: 15px;
      color: #333;
    }
    canvas {
      max-height: 300px;
    }
    .active-users {
      background: white;
      border-radius: 8px;
      padding: 20px;
      box-shadow: 0 2px 10px rgba(0,0,0,0.1);
      margin-top: 20px;
    }
    .active-users-title {
      font-size: 16px;
      font-weight: bold;
      margin-bottom: 15px;
      color: #333;
    }
    .user-tags {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
    }
    .tag {
      background: #f0f0f0;
      color: #333;
      padding: 6px 12px;
      border-radius: 20px;
      font-size: 12px;
    }
    .tag.active {
      background: #667eea;
      color: white;
    }
    .error-msg {
      background: #fee;
      color: #c33;
      padding: 15px;
      border-radius: 8px;
      margin: 20px 0;
    }
  </style>
</head>
<body>
  <div class="container">
    <header>
      <h1>📊 管理面板</h1>
      <p>实时监控 DeepSeek 代理服务</p>
    </header>

    <div id="error" class="error-msg" style="display:none;"></div>

    <div id="content">
      <div class="grid" id="cards"></div>
      <div class="charts">
        <div class="chart-container">
          <div class="chart-title">📈 登录统计 (累计)</div>
          <canvas id="loginChart" width="400" height="200"></canvas>
        </div>
      </div>
      <div class="active-users">
        <div class="active-users-title">👥 近60分钟活跃用户</div>
        <div class="user-tags" id="activeUsers"></div>
      </div>
    </div>
  </div>

  <script>
    const CONFIG = {
      apiUrl: '/api/overview',
      updateInterval: 5000,
      historyLimit: 60
    };

    const state = {
      history: [],
      loginChart: null,
  quotaChart: null // deprecated
    };

    // 获取数据
    async function fetchData() {
      try {
        const response = await fetch(CONFIG.apiUrl);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const json = await response.json();
        if (!json.success) throw new Error(json.error || '未知错误');
        
        return {
          data: json.data,
          timestamp: new Date()
        };
      } catch (error) {
        console.error('获取数据失败:', error);
        throw error;
      }
    }

    // 创建/更新登录图表
    function updateLoginChart(data) {
      const canvas = document.getElementById('loginChart');
      const ctx = canvas.getContext('2d');
      
      const success = data.data.metrics.login_success;
      const failure = data.data.metrics.login_failure;
      
      // 简单柱状图
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.fillStyle = '#27ae60';
      ctx.fillRect(20, 120, 150, success);
      ctx.fillStyle = '#e74c3c';
      ctx.fillRect(220, 120, 150, failure);
      
      ctx.fillStyle = '#333';
      ctx.font = '12px Arial';
      ctx.textAlign = 'center';
      ctx.fillText('成功: ' + success, 95, 145);
      ctx.fillText('失败: ' + failure, 295, 145);
    }


    // 更新卡片
    function updateCards(data) {
      const container = document.getElementById('cards');
      // 防御性：如果后端还没返回 cost_estimate，构造默认对象
      const ce = (data.data.metrics && data.data.metrics.cost_estimate) ? data.data.metrics.cost_estimate : {hit_cost:0, miss_cost:0, output_cost:0, total_cost:0};
      
      const cards = [
        { label: '登录成功', value: data.data.metrics.login_success, className: 'success' },
        { label: '登录失败', value: data.data.metrics.login_failure, className: 'warning' },
        { label: '暴力破解阻断', value: data.data.metrics.bruteforce_blocked, className: 'alert' },
        { label: '限流拒绝', value: data.data.metrics.rate_limit_reject, className: 'warning' },
        { label: '聊天请求', value: data.data.metrics.chat_success, className: 'success' },
  // 配额相关已移除
        { label: '今日输入Tokens', value: data.data.metrics.today_input_tokens, className: 'warning' },
        { label: '今日输出Tokens', value: data.data.metrics.today_output_tokens, className: 'warning' },
        { label: '缓存命中Tokens', value: data.data.metrics.today_cache_hit_tokens, className: 'success' },
        { label: '缓存未命中Tokens', value: data.data.metrics.today_cache_miss_tokens, className: 'warning' },
        { label: '今日预估费用(¥)', value: ce.total_cost.toFixed(2), className: 'alert' },
        { label: '费用明细(¥)', value: `${ce.hit_cost.toFixed(2)} / ${ce.miss_cost.toFixed(2)} / ${ce.output_cost.toFixed(2)}`, className: 'warning', extra: '命中 / 未命中 / 输出' }
      ];

      container.innerHTML = cards.map(card => `
        <div class="card ${card.className}">
          <div class="card-label">${card.label}${card.extra ? ' <span style="color:#999;font-weight:normal">'+card.extra+'</span>' : ''}</div>
          <div class="card-value">${typeof card.value === 'number' ? Math.round(card.value) : card.value}</div>
        </div>
      `).join('');
    }

    // 更新活跃用户
    function updateActiveUsers(data) {
      const container = document.getElementById('activeUsers');
      const users = data.data.active_users || [];
      container.innerHTML = users.length === 0 
        ? '<span class="tag">暂无活跃用户</span>'
        : users.map(user => `<span class="tag active">${user}</span>`).join('');
    }

    // 主更新循环
    async function update() {
      try {
        document.getElementById('error').style.display = 'none';
        
        const data = await fetchData();
        
        updateCards(data);
        updateLoginChart(data);
        updateActiveUsers(data);
        
      } catch (error) {
        const errorEl = document.getElementById('error');
        errorEl.textContent = `❌ 数据加载失败: ${error.message}`;
        errorEl.style.display = 'block';
      }
    }

    // 初始化
    window.addEventListener('DOMContentLoaded', () => {
      update();
      setInterval(update, CONFIG.updateInterval);
    });
  </script>
</body>
</html>
"""

# ============ 后端逻辑 ============
def parse_prometheus_metrics(text: str) -> dict:
    """解析 Prometheus 格式指标"""
    result = {}
    for line in text.split('\n'):
        if line.startswith('#') or not line.strip():
            continue
        
        # 有标签的指标: metric_name{label="value"} value
        match = re.match(r'(\w+)\{([^}]+)\}\s+([\d.eE+-]+)', line)
        if match:
            name, labels_str, value = match.groups()
            labels = {}
            for pair in labels_str.split(','):
                k, v = pair.split('=', 1)
                labels[k.strip()] = v.strip('"')
            
            if name not in result:
                result[name] = {}
            key = tuple(sorted(labels.items()))
            result[name][key] = float(value)
        else:
            # 无标签的指标: metric_name value
            match = re.match(r'(\w+)\s+([\d.eE+-]+)', line)
            if match:
                name, value = match.groups()
                result[name] = float(value)
    
    return result


def extract_metric_value(metrics: dict, name: str, label_key: str = None, label_value: str = None) -> float:
    """从解析后的指标中提取值"""
    if name not in metrics:
        return 0.0
    
    if label_key is None:
        return metrics[name] if isinstance(metrics[name], (int, float)) else 0.0
    
    metric_dict = metrics[name]
    if not isinstance(metric_dict, dict):
        return 0.0
    
    for key, val in metric_dict.items():
        if isinstance(key, tuple):
            for k, v in key:
                if k == label_key and v == label_value:
                    return val
    
    return 0.0




def get_active_users(minutes: int = 60) -> list:
    """获取最近N分钟活跃的用户"""
    cutoff = datetime.utcnow().timestamp() - minutes * 60
    active = []
    
    if not LOGS_DIR.exists():
        return active
    
    for user_dir in LOGS_DIR.iterdir():
        if not user_dir.is_dir():
            continue
        
        log_files = sorted(user_dir.glob('*.log'), reverse=True)
        if not log_files:
            continue
        
        try:
            lines = log_files[0].read_text().splitlines()[-100:]
            for line in reversed(lines):
                try:
                    obj = json.loads(line)
                    ts = obj.get('timestamp') or obj.get('time')
                    if isinstance(ts, (int, float)) and ts >= cutoff:
                        active.append(user_dir.name)
                        break
                except Exception:
                    continue
        except Exception:
            pass
    
    return active


# ============ 路由 ============
@app.get("/")
async def root():
    """返回前端 HTML"""
    from fastapi.responses import HTMLResponse
    return HTMLResponse(FRONTEND_HTML)


@app.get("/api/overview")
async def get_overview():
  """获取完整概览数据"""
  try:
    async with httpx.AsyncClient(timeout=5.0) as client:
      resp = await client.get(METRICS_URL)
      metrics_text = resp.text if resp.status_code == 200 else ""

    metrics = parse_prometheus_metrics(metrics_text)
    active_users = get_active_users(60)

    login_success = extract_metric_value(metrics, 'login_attempts_total', 'result', 'success')
    login_failure = extract_metric_value(metrics, 'login_attempts_total', 'result', 'failure')
    bruteforce_blocked = extract_metric_value(metrics, 'login_bruteforce_blocked_total')
    rate_limit_reject = extract_metric_value(metrics, 'rate_limit_rejections_total')
    chat_success = extract_metric_value(metrics, 'chat_requests_total', 'status', 'success')
    quota_exceeded = extract_metric_value(metrics, 'quota_checks_total', 'status', 'exceeded')
    today_input_tokens = extract_metric_value(metrics, 'today_input_tokens')
    today_output_tokens = extract_metric_value(metrics, 'today_output_tokens')
    today_cache_hit_tokens = extract_metric_value(metrics, 'today_prompt_cache_hit_tokens')
    today_cache_miss_tokens = extract_metric_value(metrics, 'today_prompt_cache_miss_tokens')

    hit_tokens = today_cache_hit_tokens
    miss_tokens = today_cache_miss_tokens
    if hit_tokens == 0 and miss_tokens == 0 and today_input_tokens > 0:
      miss_tokens = today_input_tokens

    hit_cost = hit_tokens / 1_000_000 * 0.2
    miss_cost = miss_tokens / 1_000_000 * 2.0
    output_cost = today_output_tokens / 1_000_000 * 3.0
    total_cost = hit_cost + miss_cost + output_cost

    return JSONResponse({
      "success": True,
      "data": {
        "metrics": {
          "login_success": int(login_success),
          "login_failure": int(login_failure),
          "bruteforce_blocked": int(bruteforce_blocked),
          "rate_limit_reject": int(rate_limit_reject),
          "chat_success": int(chat_success),
          "quota_exceeded": int(quota_exceeded),
          "today_input_tokens": int(today_input_tokens),
          "today_output_tokens": int(today_output_tokens),
          "today_cache_hit_tokens": int(today_cache_hit_tokens),
          "today_cache_miss_tokens": int(today_cache_miss_tokens),
          "cost_estimate": {
            "hit_cost": round(hit_cost, 6),
            "miss_cost": round(miss_cost, 6),
            "output_cost": round(output_cost, 6),
            "total_cost": round(total_cost, 6)
          }
        },
        "active_users": active_users,
      }
    })
  except Exception as e:
    return JSONResponse({"success": False, "error": str(e)}, status_code=500)


if __name__ == "__main__":
    import uvicorn
    print("=" * 50)
    print("🚀 DeepSeek 管理面板已启动")
    print("=" * 50)
    print(f"📊 前端: http://127.0.0.1:8089")
    print(f"📡 后端: http://127.0.0.1:8089/api/overview")
    print(f"🔗 代理: {PROXY_URL}")
    print("=" * 50)
    uvicorn.run(app, host="127.0.0.1", port=8089, log_level="warning")
