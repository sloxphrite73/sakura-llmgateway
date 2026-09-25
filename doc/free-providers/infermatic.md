# Infermatic — 免费额度领取教程

> Base URL: `https://api.totalgpt.ai/v1` · 协议: OpenAI 兼容（注意：官网域名为 infermatic.ai，API 域名为 api.totalgpt.ai）

## 1. 注册账号

1. 打开 [https://infermatic.ai](https://infermatic.ai/)
2. 注册账号，选择 **Free 计划**（无需绑卡即可开始）

## 2. 创建 API Key

1. 登录后在控制台/Dashboard 创建 API Key
2. 复制保存（作 Bearer 鉴权）

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Infermatic 卡片，粘贴你的 Key（多个 Key 每行一个）
3. 点「确认添加」——预置的免费模型会自动导入

## 4. 免费模型说明

- 免费计划含专属 **Free 模型档**，例如：
  - `TheDrummer-Rocinante-12B-v1.1`
  - `Sao10K-L3.3-70B-Euryale-v2.3-FP8-Dynamic`
- 另有 $9 / $20 付费档（Flat-rate gateway，覆盖更多模型）

## 5. 注意事项

- 真实域名为 `infermatic.ai`（API 主机 `api.totalgpt.ai`）；`infermatic.com` 不是官方域名
- 走 OpenAI 兼容端点：`POST https://api.totalgpt.ai/v1/chat/completions`（vLLM 后端）
- 免费档仅限 Free 模型；调用付费模型需升级套餐
- 网关的智能冷却会自动处理 429
