# Cloudflare Workers AI — 免费额度领取教程

> Base URL: `https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1` · 协议: OpenAI 兼容
>
> ⚠️ **base_url 里的 `{account_id}` 必须替换成你自己的账户 ID**，添加时在卡片里改好。

## 1. 注册账号

1. 打开 [https://dash.cloudflare.com](https://dash.cloudflare.com/sign-up)
2. 免费注册，无需信用卡

## 2. 获取 account_id 和 API Token

1. 登录后进入任意域名/Workers 页，右侧栏可见 **Account ID**（或 Workers & Pages → 右侧「Account details」）→ 复制
2. 进入 [API Tokens 页面](https://dash.cloudflare.com/profile/api-tokens) → 「Create Token」→ 用「Create Custom Token」给 **Workers AI** 读权限 → 生成并复制

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Cloudflare 卡片
3. **先把 base_url 中的 `{account_id}` 替换成第 2 步复制的账户 ID**，再粘贴 Token
4. 点「确认添加」——模型自动导入

## 4. 免费模型说明

- 每天 **10000 neurons** 免费额度（按模型/输入输出量折算）
- 目录预置：
  - `@cf/meta/llama-3.1-8b-instruct`
  - `@cf/qwen/qwen1.5-14b-chat-awq`
  - `@cf/mistralai/mistral-7b-instruct-v0.1`
- 完整模型列表见 [Cloudflare 文档](https://developers.cloudflare.com/workers-ai/models/)

## 5. 注意事项

- neurons 按天重置，小模型足够日常轻量使用
- Token 权限只给 Workers AI 必要权限，最小化暴露面
