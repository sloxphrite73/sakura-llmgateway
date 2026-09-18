# Fireworks AI — 免费额度领取教程

> Base URL: `https://api.fireworks.ai/inference/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [Fireworks 注册页](https://fireworks.ai/signup)
2. 邮箱或社交账号自助注册，无需绑卡即可开始

## 2. 创建 API Key

1. 登录后在 [app.fireworks.ai](https://app.fireworks.ai) 控制台创建 API key
2. 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Fireworks AI 卡片，粘贴你的 Key
3. 点「确认添加」——免费模型自动导入

## 4. 免费模型说明

- 新账户一次性获赠 $1 免费额度，可用于任意按量计费的 serverless 模型
- 目录预置模型：
  - `accounts/fireworks/models/glm-5p2`
  - `accounts/fireworks/models/gpt-oss-120b`
  - `accounts/fireworks/models/deepseek-v4-flash-0731`
- 模型 id 必须带 `accounts/fireworks/models/` 前缀

## 5. 注意事项

- $1 额度一次性、不循环，用尽需添加付款方式购买更多额度
- base_url 是 `/inference/v1`（不是 `/v1`），网关已按官方配置
- 模型与价格以 [Fireworks 定价页](https://docs.fireworks.ai/serverless/pricing) 为准
