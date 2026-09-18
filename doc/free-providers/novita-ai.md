# Novita AI — 免费额度领取教程

> Base URL: `https://api.novita.ai/openai` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [Novita 控制台](https://novita.ai/console)
2. 注册登录，无需绑卡

## 2. 创建 API Key

1. 在控制台创建 API key
2. 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Novita AI 卡片，粘贴你的 Key
3. 点「确认添加」——免费模型自动导入

## 4. 免费模型说明

- 注册即赠 $1 信用额度（无需绑卡），可用于任意模型
- 另有 $0「限时免费」模型：
  - `inclusionai/ling-3.0-flash-fin`（124B MoE，金融增强，$0 输入/输出）
  - `inclusionai/ling-3.0-flash-sante`
- 模型 id 前缀是小写 `inclusionai/`

## 5. 注意事项

- 限时免费促销模型可能随时下线，下线后改用 $1 额度或付费
- base_url 为 `https://api.novita.ai/openai`（官方 SDK 路径，网关自动追加 `/chat/completions`）
- 模型与价格以 [novita.ai/models](https://novita.ai/models) 为准
