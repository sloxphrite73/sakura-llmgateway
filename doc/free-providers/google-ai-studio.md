# Google AI Studio (Gemini) — 免费额度领取教程

> Base URL: `https://generativelanguage.googleapis.com/v1beta/openai/` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [Google AI Studio](https://aistudio.google.com)
2. 用 Google 账号登录，无需信用卡

## 2. 创建 API Key

1. 进入 [API Key 页面](https://aistudio.google.com/apikey)
2. 点「Create API key」→ 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Google AI Studio 卡片，粘贴你的 Key
3. 点「确认添加」——免费模型自动导入

## 4. 免费模型说明

- 免费层 `gemini-3.x-flash` 系列 $0，按每分钟请求数（RPM）+ 每日请求数（RPD）限额，每日太平洋时间午夜重置
- 目录预置模型：
  - `gemini-3.8-flash`
  - `gemini-3.7-flash`
  - `gemini-3.6-flash`
  - `gemini-3.5-flash`
- 免费层默认开启数据用于改进 Google 产品，介意可在项目设置关闭

## 5. 注意事项

- base_url 以 `/v1beta/openai/` 结尾（不是 `/v1`），网关已按官方文档原样配置，勿改
- 重度突发会触发 429，建议与 SiliconFlow / SambaNova / Groq 组成多供应商池
- 模型版本会轮换，以 [ai.google.dev/gemini-api/docs/models](https://ai.google.dev/gemini-api/docs/models) 为准
