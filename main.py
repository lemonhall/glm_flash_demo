from glm_client import GLMClient


def main():
    """流式调用示例"""
    # 自动从环境变量 GLM_FLASH_API_KEY 读取 API Key
    with GLMClient() as client:
        print("开始对话...\n")
        
        # 流式输出
        for text in client.chat(
            messages=[
                {"role": "system", "content": "你是一个有用的AI助手。"},
                {"role": "user", "content": "写一首关于秋天的小诗"}
            ],
            model="glm-4.5-flash",
            temperature=0.95
        ):
            print(text, end="", flush=True)
        
        print("\n\n对话结束。")


if __name__ == "__main__":
    main()
