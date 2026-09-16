# Groq — 免费额度领取教程

> Base URL: `https://api.groq.com/openai/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [https://console.groq.com](https://console.groq.com)
2. Google 或邮箱登录，无需信用卡

## 2. 创建 API Key

1. 进入 [API Keys 页面](https://console.groq.com/keys)
2. 点「Create API Key」→ 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Groq 卡片，粘贴你的 Key
3. 点「确认添加」——免费模型自动导入

## 4. 免费模型说明

- 免费层限额按**每分钟请求数（RPM）+ 每日请求数（RPD）**双轨计算，不用充值
- 目录预置模型：
  - `llama-3.3-70b-versatile`
  - `llama-3.1-8b-instant`
  - `openai/gpt-oss-120b`
  - `qwen/qwen3-32b`
  - `moonshotai/kimi-k2-instruct`
- Groq 的 LPU 推理速度极快，Llama 系列尤其适合做高速生产者

## 5. 注意事项

- 免费层日请求量有限，超过后返回 429，建议与 SiliconFlow / Cerebras 组成多供应商池
- 模型列表以 [console.groq.com/docs/models](https://console.groq.com/docs/models) 为准
