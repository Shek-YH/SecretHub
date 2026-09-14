# Provider catalog review — 2026-09-14

SecretHub 的服务商目录以 [EchoBird 的 `modelDirectory.json`](https://github.com/edison7009/EchoBird/blob/main/src/data/modelDirectory.json) 为迁移基线，包含直连服务商、国内/海外区域入口及中转服务商；每个条目都保留 API Key 官网和 OpenAI-compatible Base URL，模型仍支持自定义 ID。

本次重点核对的官方生命周期资料：

- [DeepSeek Models & Pricing](https://api-docs.deepseek.com/quick_start/pricing)：`deepseek-chat`、`deepseek-reasoner` 进入弃用迁移，模板改为 `deepseek-v4-flash`、`deepseek-v4-pro`，并保留视觉实验模型入口。
- [DeepSeek API Updates](https://api-docs.deepseek.com/updates/)：V4 Flash 使用原有 API 调用方式，仅切换模型 ID。
- [Anthropic Models Overview](https://platform.claude.com/docs/en/models/overview)：当前模板使用 Claude Fable 5.1、Opus 5、Sonnet 5、Haiku 4.5。
- [Google Gemini Models](https://ai.google.dev/gemini-api/docs/models)：模板切换到 Gemini 3 系列，同时保留自定义模型入口。
- [xAI Models](https://docs.x.ai/developers/models)：当前主推 Grok 4.6，并保留 Grok 4.x 兼容项。
- [Mistral Models](https://docs.mistral.ai/models)：模板使用 Mistral Medium 3.5、Small 4、Large 3 和 Codestral 等当前系列。
- [Groq Supported Models](https://console.groq.com/docs/models)：移除旧的 Llama 生产项，改为 GPT-OSS、Qwen 3.6/3.8、MiniMax M2.7 和 Compound 系列。
- [腾讯 TokenHub 语言模型调用概览](https://cloud.tencent.com/document/product/1823/130079)：混元模板迁移到 Hy3/Hy4 Preview 与 TokenHub 聚合模型。

中转平台模型目录可能按账户、套餐或平台实时变化，因此模板保留“自定义模型 ID”；后续可增加带 API Key 的本地原生 `/models` 刷新，不把 Key 暴露给 WebView。
