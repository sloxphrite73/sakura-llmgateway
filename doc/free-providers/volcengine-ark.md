# 火山方舟 Volcengine Ark (Doubao) — 免费额度领取教程

> Base URL: `https://ark.cn-beijing.volces.com/api/v3` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [火山方舟控制台](https://console.volcengine.com/ark)
2. 用火山引擎账号登录并完成实名认证（身份认证，非信用卡）

## 2. 获取 API Key

1. 在控制台「获取 API Key 并配置」创建 ARK_API_KEY
2. 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到 Volcengine Ark 卡片，粘贴你的 Key
3. 点「确认添加」——预置模型自动导入

## 4. 免费模型说明

- 新用户实名认证后享「安心体验模式」免费额度（无需绑卡，耗尽即停），每个模型另有免费调用额度
- 目录预置模型（API id，带日期后缀）：
  - `doubao-seed-2-1-pro-260628`
  - `doubao-seed-2-0-lite-260428`

## 5. 注意事项

- **模型 id 带日期后缀且频繁轮换**——添加后请到火山控制台「模型列表」核对当前可用 id，必要时手动改目录条目
- base_url 为 `/api/v3`（不是 `/v1`），网关已按官方文档配置，勿改
- 需在火山控制台先「开通」对应模型才能调用
- 当前可用模型以 [方舟模型列表](https://docs.volcengine.com/docs/82379/1330310) 为准
