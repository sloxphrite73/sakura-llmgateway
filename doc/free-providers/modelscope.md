# ModelScope (魔搭社区) — 免费额度领取教程

> Base URL: `https://api-inference.modelscope.cn/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [魔搭社区 ModelScope](https://www.modelscope.cn/)
2. 注册并登录，**即送 200 Magicubes**；绑定阿里云账号每日再续 50 个（**无需绑卡**）

## 2. 创建 API Key

1. 在「访问令牌 / API Token」页面创建令牌
2. 复制保存（作 Bearer 鉴权）

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 ModelScope 卡片，粘贴你的 Token（多个每行一个）
3. 点「确认添加」——预置的免费模型会自动导入

## 4. 免费模型说明

- API-Inference 免费提供 1000+ 开源模型推理，例如：
  - `deepseek-ai/DeepSeek-R1`
  - `Qwen/Qwen2.5-72B-Instruct`
  - `deepseek-ai/DeepSeek-V3`
- 模型 id 为 `org/model` 格式，阵容会轮换，建议添加后从上游 `/v1/models` 重新拉取当前列表

## 5. 注意事项

- ModelScope 与已收录的 **DashScope/百炼 是不同服务**：这里免费托管开源模型，百炼是阿里官方付费 API
- 走 OpenAI 兼容端点：`POST https://api-inference.modelscope.cn/v1/chat/completions`
- 免费额度以 Magicubes 计量，每日续赠；网关的智能冷却会自动处理 429
