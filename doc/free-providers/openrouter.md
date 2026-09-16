# OpenRouter :free — 免费模型教程

> Base URL: `https://openrouter.ai/api/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [https://openrouter.ai](https://openrouter.ai)
2. 邮箱或 Google/Discord 登录

## 2. 创建 API Key

1. 进入 [Keys 页面](https://openrouter.ai/keys)
2. 点「Create Key」→ 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 OpenRouter 卡片，粘贴你的 Key
3. 点「确认添加」——预置的 `:free` 模型自动导入

## 4. 免费模型说明

- 所有带 **`:free`** 后缀的模型免费调用，目录里预置了：
  - `deepseek/deepseek-chat-v3.1:free`
  - `deepseek/deepseek-r1:free`
  - `qwen/qwen3-coder:free`
  - `moonshotai/kimi-k2:free`
  - `meta-llama/llama-3.3-70b-instruct:free`
  - `google/gemini-2.0-flash-exp:free`
- 免费模型每日请求数有限（未充值约 50 次/天）
- **充值 $10 后**免费限额提升到 1000 次/天（充的钱本身仍可用，相当于解锁）

## 5. 注意事项

- 完整免费模型列表见 [openrouter.ai/models?q=free](https://openrouter.ai/models?q=free)，可手动追加到网关
- `:free` 模型上下文长度各不相同，长文任务注意选择
