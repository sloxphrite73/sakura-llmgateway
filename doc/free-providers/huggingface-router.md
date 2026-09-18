# Hugging Face Router — 免费额度领取教程

> Base URL: `https://router.huggingface.co/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [Hugging Face](https://huggingface.co)，注册登录

## 2. 创建 Access Token

1. 进入 [Access Tokens 设置页](https://huggingface.co/settings/tokens)
2. 新建 token（fine-grained，勾选「Make calls to Inference Providers」权限）→ 复制

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Hugging Face Router 卡片，粘贴你的 token
3. 点「确认添加」——免费模型自动导入

## 4. 免费模型说明

- 免费用户每月有循环免费额度（无需支付方式），可用于所有 Inference Providers 聊天模型，每月重置
- 目录预置模型：
  - `openai/gpt-oss-120b`
  - `deepseek-ai/DeepSeek-R1`
- 模型 id 可加路由后缀：`:fastest`（默认）/`:cheapest`/`:preferred`

## 5. 注意事项

- 免费额度较小（适合试用，不适合生产）
- `/v1` 端点仅支持 chat completions（不含 embeddings/images）
- 模型目录会变动，以 [HF Inference Providers 文档](https://huggingface.co/docs/inference-providers/index) 为准
