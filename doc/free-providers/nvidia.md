# NVIDIA NIM — 免费额度领取教程

> Base URL: `https://integrate.api.nvidia.com/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [https://build.nvidia.com](https://build.nvidia.com)
2. 用 NVIDIA 账号登录（没有就免费注册一个）

## 2. 获取 API Key

1. 在任意模型页面点「Get API Key」（或直接访问 build.nvidia.com 个人设置）
2. 生成后复制保存——**注册即送 1000 credits**
3. 1 credit = 1 次请求（不是按 token 计费）

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 NVIDIA NIM 卡片，粘贴你的 Key
3. 点「确认添加」——模型自动导入

## 4. 免费模型说明

- 1000 credits 可在**几十款模型**间通用，目录预置：
  - `deepseek-ai/deepseek-r1`
  - `qwen/qwen3-coder-480b-a35b-instruct`
  - `meta/llama-3.3-70b-instruct`
  - `mistralai/mixtral-8x22b-instruct-v0.1`
- 完整列表见 [build.nvidia.com](https://build.nvidia.com/models)，可手动追加

## 5. 注意事项

- credits 用完后每月刷新（约 1000/月），轻量使用够用
- 单次请求不限模型档次，DeepSeek-R1 这类大杯推理模型也能用 credit 调
