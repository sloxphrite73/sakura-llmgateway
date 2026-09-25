# Baidu ERNIE / Qianfan (百度千帆) — 免费额度领取教程

> Base URL: `https://qianfan.baidubce.com/v2` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [百度智能云千帆](https://cloud.baidu.com/product-s/qianfan_home)
2. 注册百度智能云账号并完成**实名认证**（个人/企业认证，**无需绑卡**）

## 2. 创建 API Key

1. 进入千帆 ModelBuilder 控制台，开通服务
2. 在「应用/API Key」页面创建 API Key（千帆 API Key，形如 `bce-v3/...`，作 Bearer 鉴权）

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到百度千帆卡片，粘贴你的 Key（多个 Key 每行一个）
3. 点「确认添加」——预置的免费模型会自动导入

## 4. 免费模型说明

- 新用户实名认证后赠 **¥20 代金券**（全平台无门槛、1 个月有效），可免费调用：
  - `ernie-4.5-turbo-32k`
  - `ernie-5.0`
  - `deepseek-v4.1-flash`
- 代金券到期后按量付费

## 5. 注意事项

- 代金券仅 **1 个月有效**，用尽/到期后转为按量付费
- 模型阵容会随百度更新轮换（ernie-5.x、deepseek-v4.x 等），建议添加后从上游 `/v2/models` 重新拉取当前列表
- 走 OpenAI 兼容端点：`POST https://qianfan.baidubce.com/v2/chat/completions`，鉴权 `Authorization: Bearer <千帆 API Key>`
