"""
基于 httpx 的智谱 AI GLM API 客户端 - 仅支持同步流式调用
"""
import json
import os
from typing import Any, Dict, List, Optional, Iterator
import httpx


class GLMClient:
    """智谱 AI GLM API 同步流式客户端"""
    
    BASE_URL = "https://open.bigmodel.cn/api/paas/v4"
    
    def __init__(self, api_key: Optional[str] = None, timeout: float = 60.0):
        """
        初始化客户端
        
        Args:
            api_key: API 密钥，如不提供则从环境变量 GLM_FLASH_API_KEY 读取
            timeout: 请求超时时间(秒)
        """
        if api_key is None:
            api_key = os.getenv("GLM_FLASH_API_KEY")
            if not api_key:
                raise ValueError("未提供 API Key，请设置环境变量 GLM_FLASH_API_KEY 或传入 api_key 参数")
        
        self.api_key = api_key
        self.timeout = timeout
        self.client = httpx.Client(
            base_url=self.BASE_URL,
            timeout=timeout,
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json"
            }
        )
    
    def chat(
        self,
        messages: List[Dict[str, str]],
        model: str = "glm-4.5-flash",
        temperature: float = 1.0,
        top_p: float = 0.95,
        max_tokens: Optional[int] = None,
        **kwargs
    ) -> Iterator[str]:
        """
        流式聊天接口
        
        Args:
            messages: 消息列表，格式: [{"role": "user", "content": "..."}]
            model: 模型名称，默认 glm-4.5-flash
            temperature: 采样温度 0.0-1.0，默认 1.0
            top_p: 核采样参数，默认 0.95
            max_tokens: 最大输出 token 数
            **kwargs: 其他参数 (do_sample, stop, request_id, user_id 等)
            
        Yields:
            str: 流式返回的文本内容片段
        """
        payload = {
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "top_p": top_p,
            "stream": True,
            **kwargs
        }
        
        if max_tokens is not None:
            payload["max_tokens"] = max_tokens
        
        with self.client.stream("POST", "/chat/completions", json=payload) as response:
            response.raise_for_status()
            
            for line in response.iter_lines():
                if not line.strip():
                    continue
                
                # SSE 格式: "data: {json}"
                if line.startswith("data: "):
                    data_str = line[6:]  # 去掉 "data: " 前缀
                    
                    # 流式结束标志
                    if data_str.strip() == "[DONE]":
                        break
                    
                    try:
                        chunk = json.loads(data_str)
                        
                        # 提取流式返回的文本内容
                        if "choices" in chunk and len(chunk["choices"]) > 0:
                            delta = chunk["choices"][0].get("delta", {})
                            content = delta.get("content", "")
                            if content:
                                yield content
                                
                    except json.JSONDecodeError:
                        continue
    
    def close(self):
        """关闭客户端"""
        self.client.close()
    
    def __enter__(self):
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
