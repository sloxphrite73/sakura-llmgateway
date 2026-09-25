# Tencent Hunyuan (腾讯混元) — 免费额度领取教程

> Base URL: `https://api.hunyuan.cloud.tencent.com/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [腾讯云混元大模型](https://cloud.tencent.com/document/product/1729)
2. 注册腾讯云账号并完成**实名认证**（个人/企业认证，**无需绑卡**）

## 2. 创建 API Key

1. 在混元控制台首次开通服务，即获**免费资源包**：生文模型共享 100 万 tokens（1 年有效）+ Hunyuan-embedding 100 万 tokens
2. 在「API Key 管理」创建密钥（作 Bearer 鉴权）

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到腾讯混元卡片，粘贴你的 Key（多个 Key 每行一个）
3. 点「确认添加」——预置的免费模型会自动导入

## 4. 免费模型说明

- 免费资源包可调用生文模型：
  - `hunyuan-a13b`
  - `hunyuan-role-latest`
  - `hunyuan-translation`
  - `hunyuan-translation-lite`
- 100 万 tokens 共享，1 年有效，用尽前不会扣费

## 5. 注意事项

- 走 OpenAI 兼容端点：`POST https://api.hunyuan.cloud.tencent.com/v1/chat/completions`
- 官方公告混元功能正**逐步迁移至 TokenHub**；原平台已停新购，但**免费体验额度仍按首次开通发放**（计费页更新于 2026-06-26）
- 建议添加后确认免费额度仍可领取；模型阵容迁移期间可能变动，从上游 `/v1/models` 重新拉取当前列表
