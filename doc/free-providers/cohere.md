# Cohere — 免费额度领取教程

> Base URL: `https://api.cohere.ai/compatibility/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [Cohere Dashboard](https://dashboard.cohere.com)
2. 注册登录，无需信用卡

## 2. 创建 API Key

1. 进入 [API Keys 页面](https://dashboard.cohere.com/api-keys)
2. 创建 trial key → 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Cohere 卡片，粘贴你的 Key
3. 点「确认添加」——免费模型自动导入

## 4. 免费模型说明

- 免费 trial key 每月 1,000 次调用、每模型 20 req/min，无需绑卡
- 目录预置模型：
  - `command-a-plus-05-2026`
  - `command-a-03-2025`
  - `command-r-08-2024`
  - `command-r-plus-08-2024`
- 走 OpenAI 兼容端点（Compatibility API）

## 5. 注意事项

- base_url 是 `https://api.cohere.ai/compatibility/v1`（注意 `.ai` 域 + `/compatibility/v1` 路径，不是 `/v1`）
- 旧的 `command-r` / `command-r-plus` 别名已弃用，请用带日期版本
- 兼容端点丢弃部分 OpenAI 参数（`store`/`n`/`logit_bias` 等），`reasoning_effort` 仅支持 `none`/`high`
- 模型列表以 [docs.cohere.com/docs/models](https://docs.cohere.com/docs/models) 为准
