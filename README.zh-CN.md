# Sakura LLM Gateway

高速、便携、智能、本地运行的大模型网关。

[English](README.md) | [简体中文](README.zh-CN.md) · 使用说明：[中文](doc/使用说明.md) | [English](doc/USAGE.md)

反馈 Bug 和功能建议进 QQ 群 348900172

## 功能亮点

+ 强大的免费额度限流适应能力，支持多指标排序的灵活轮询策略。
+ 支持 25 家免费额度上游提供商，内置教程。
+ 同时支持 Windows 和 Android 使用，单文件 0 依赖。
+ 支持多提供商和 API Key 的便捷管理。
+ 支持 Anthropic Messages 和 OpenAI Compatible 双协议接入和互相翻译。
+ 支持查看全方位（总览/按Key/按模型/按提供商）多维统计指标，支持查看过去 24 小时用量/请求情况。


## 快速开始

在 [Release](https://github.com/sloxphrite73/sakura-llmgateway/releases) 页下载符合自身系统版本的二进制文件即可。

关于项目本身如何使用请参阅[使用说明](doc/使用说明.md)。

| 服务 | 地址 |
|------|------|
| OpenAI 兼容 API | `http://127.0.0.1:8000/v1` |
| Anthropic API | `http://127.0.0.1:8000/v1/messages` |
| Web 控制台 | `http://127.0.0.1:8001/` |

默认端口（API `8000`、控制台 `8001`）可在配置文件中设置，也可在 Web 控制台里修改。

## 技术栈

- **Rust**（edition 2021）
- **axum + tokio + hyper**
- **reqwest (rustls)** 
- **rust-embed** 
- **serde / serde_json** 

## 开发

有意加入共同开发者请联系作者 QQ:2183298801。

本项目不接受在与作者沟通交流之前的 Pull Request。

使用或基于本项目进行进一步开发请参阅 [开发指南](doc/开发指南.md)。

## 许可证

[PolyForm Noncommercial 1.0.0](LICENSE) 


**署名要求：** 直接/间接使用本项目源代码的开源项目，必须在其文档中显式标出作者（**sloxphrite73**）和本项目名称（**sakura-llmgateway**）。本项目不接受商用和非开源使用。

注：v0.8.5-stable 及更早版本以 GPL-3.0 发布，已发布的副本仍按 GPL-3.0 授权；之后的版本适用 PolyForm Noncommercial 1.0.0。

