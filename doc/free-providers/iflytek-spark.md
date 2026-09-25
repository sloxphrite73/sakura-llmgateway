# iFlyTek Spark (讯飞星火) — 免费额度领取教程

> Base URL: `https://spark-api-open.xf-yun.com/v1` · 协议: OpenAI 兼容

## 1. 注册账号

1. 打开 [讯飞开放平台星火](https://xinghuo.xfyun.cn/sparkapi)
2. 注册并完成**实名认证**（个人认证，**无需绑卡**）

## 2. 创建 API Key

1. 在星火控制台获取 **APIPassword**（OpenAI 兼容端点用它作 Bearer token，区别于旧版 APPID/APIKey/APISecret 三件套）
2. 复制保存

## 3. 回到网关添加

1. 打开网关控制台 → 配置页 → 「现有提供商」面板
2. 找到讯飞星火卡片，粘贴你的 APIPassword（多个每行一个）
3. 点「确认添加」——预置的免费模型会自动导入

## 4. 免费模型说明

- `lite` 版**永久免费**
- 另可在产品页领取免费额度（real-name，无需绑卡），可用于：
  - `generalv3`（Pro）
  - `generalv3.5`（Max）
- 模型 id 还含 `4.0Ultra` / `max-32k` / `pro-128k`（注意 Max 套餐已于 2026-03-10 下线升级为 Ultra）

## 5. 注意事项

- 走 OpenAI 兼容端点：`POST https://spark-api-open.xf-yun.com/v1/chat/completions`，鉴权 `Authorization: Bearer <APIPassword>`
- `lite` 永久免费但能力较弱；复杂任务用 `generalv3`/`generalv3.5` 消耗领取的额度
- 模型阵容会轮换，建议添加后从上游 `/v1/models` 重新拉取当前列表
