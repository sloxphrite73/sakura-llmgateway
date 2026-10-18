# Sakura LLM Gateway

A fast, portable, smart, local-first LLM gateway.

[English](README.md) | [简体中文](README.zh-CN.md) · Usage guide: [English](doc/USAGE.md) | [中文](doc/使用说明.md)

Bugs report and communication with dev: [Telegram](https://t.me/+4NfMkc8SJDwzYWZl)

## Features

+ Strong adaptation to free-tier rate limits, with a flexible rotation strategy that sorts on multiple metrics.
+ 25 free-tier upstream providers built in, with setup guides.
+ Runs on Windows and Android — a single file with zero dependencies.
+ Convenient management of multiple providers and API keys.
+ Speaks Anthropic Messages and OpenAI-compatible protocols, with bidirectional translation.
+ Comprehensive statistics (totals / per-key / per-model / per-provider), including past-24h usage and request histograms.

## Measured in production

In a real development environment (33 Sensenova API keys pooled), Kimi K3 averaged ~4s time-to-first-token on short contexts and ~15s on 700K-token contexts (TTFT is determined by the upstream model itself; the gateway adds only milliseconds of overhead).

## Quick start

Download the binary for your platform from the [Releases](https://github.com/sloxphrite73/sakura-llmgateway/releases) page.

For how to use the gateway itself, see the [usage guide](doc/USAGE.md).

| Service | Address |
|---------|---------|
| OpenAI-compatible API | `http://127.0.0.1:8000/v1` |
| Anthropic API | `http://127.0.0.1:8000/v1/messages` |
| Web console | `http://127.0.0.1:8001/` |

Default ports (API `8000`, console `8001`) can be set in the config file or changed in the web console.

## Tech stack

- **Rust** (edition 2021)
- **axum + tokio + hyper**
- **reqwest (rustls)**
- **rust-embed**
- **serde / serde_json**

## Development

Interested in joining as a co-developer? Contact the author on [Telegram](https://t.me/+4NfMkc8SJDwzYWZl).

Pull requests are not accepted without prior communication with the author.

For further development on or based on this project, see the [development guide](doc/开发指南.md) (Chinese).

## License

[PolyForm Noncommercial 1.0.0](LICENSE) — free for personal, research, educational, and other noncommercial use. Commercial use requires a separate license from the author.

**Attribution requirement:** any open-source project that directly or indirectly uses this project's source code must explicitly credit the author (**sloxphrite73**) and this project (**sakura-llmgateway**) in its documentation. This project does not permit commercial use or closed-source use.

Note: releases up to and including v0.8.5-stable were published under GPL-3.0; those copies remain available under GPL-3.0. Later versions are under PolyForm Noncommercial 1.0.0.
