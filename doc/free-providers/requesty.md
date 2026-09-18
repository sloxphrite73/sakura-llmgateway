# Requesty (LLM Router) — 免费额度领取教程

> Base URL: `https://router.requesty.ai/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [Requesty 注册页](https://app.requesty.ai/sign-up)
2. 注册登录，无需信用卡

## 2. 创建 API Key

1. 在 [app.requesty.ai](https://app.requesty.ai) 控制台生成 API key
2. 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Requesty 卡片，粘贴你的 Key
3. 点「确认添加」——免费模型自动导入

## 4. 免费模型说明

- 免费层每日 200 次请求、20 req/min，无需信用卡、无试用到期，仅限免费模型
- 目录预置模型（均为 $0）：
  - `nvidia/nemotron-3-super-120b-a12b`
  - `google/gemma-4-31b-it`（全球可用）
  - `meta/muse-glimmer-30b`
  - `mistral/leanstral-1-5`（仅 EU）
  - `inclusionai/ling-3.0-tiny`
  - `nvidia/nemotron-3-ultra-550b-a55b`

## 5. 注意事项

- 免费模型目录会变动，以 [requesty.ai/models/free](https://www.requesty.ai/models/free) 为准
- 部分模型有区域限制（如 leanstral-1-5 仅 EU、多数 nvidia 仅 US），区域不符会 429/失败
- base_url 为 `https://router.requesty.ai/v1`（不是 api.requesty.ai）
