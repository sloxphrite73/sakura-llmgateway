# Stepfun (阶跃星辰) — 免费额度领取教程

> Base URL: `https://api.stepfun.com/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [阶跃星辰开放平台](https://platform.stepfun.com/)
2. 注册并完成**实名认证**（个人认证，**无需绑卡**）

## 2. 创建 API Key

1. 登录后在「API Key 管理」页面创建密钥
2. 复制保存（作 Bearer 鉴权）

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Stepfun 卡片，粘贴你的 Key（多个 Key 每行一个）
3. 点「确认添加」——预置的免费模型会自动导入

## 4. 免费模型说明

- 新用户赠送额度（「赠送账户」，有有效期），可调用：
  - `step-5-preview`
  - `step-3.7-flash`
  - `step-3.5-flash`
- 另有 4 个限时免费语音模型（促销可能随时结束）

## 5. 注意事项

- 赠送额度**有有效期**，用尽后需充值；充值才升档速率
- 仅赠送额度时为 **V0 速率档**（5 并发 / 100 RPM / 500K TPM）
- 走 OpenAI 兼容端点：`POST https://api.stepfun.com/v1/chat/completions`
- 网关的智能冷却会自动处理 429
