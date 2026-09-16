# Cerebras — 免费额度领取教程

> Base URL: `https://api.cerebras.ai/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [https://cloud.cerebras.ai](https://cloud.cerebras.ai)
2. 邮箱或 Google 登录（无需信用卡）

## 2. 创建 API Key

1. 登录后进入 API Keys 页面
2. 点「Create API Key」→ 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Cerebras 卡片，粘贴你的 Key
3. 点「确认添加」——免费模型自动导入

## 4. 免费模型说明

- 免费层**每天约 100 万 tokens**，按日重置
- 可用模型（以控制台实时列表为准）：
  - `llama3.1-8b` / `llama-3.3-70b`
  - `qwen-3-32b`
  - `gpt-oss-120b`
- Cerebras 使用专用推理芯片，**生成速度是商用 API 中最快的一档**（数千 tokens/秒）

## 5. 注意事项

- 免费层有 RPM 与 TPM 双限制，适合搭配多 Key 池
- 超出日额度会返回 429，网关智能冷却会自动等待到次日窗口
