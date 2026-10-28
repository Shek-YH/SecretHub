# 服务商调用方式调研（invocations）— 2026-10-28

## 1. 方法说明

- 检查日期：2026-10-28。
- 数据来源：仅限服务商官方文档站（vendor 域名）。聚合平台只采用其自有官方文档；无博客、无推测。
- 来源：`packages/provider-definitions/providers.json` 的 `id` / `baseUrl`；格式与 `kind` 枚举依据 `docs/superpowers/specs/2026-10-28-multimodal-keys-media-onboarding-design.md` §M1.5。
- `path` 一律相对于 providers.json 中该服务商的 `baseUrl`，以 `/` 开头。若官方接口位于不同主机或不同 base 路径（例如 `/api/coding/...` 套餐端点、`token-plan.*` 等），按规则标记为 `native`，不编造相对路径。
- 异步接口记录提交路径（path）与轮询路径模式（pollPath）。
- 置信度：high = 官方页面明确给出方法+路径；medium = 官方页面给出能力与基址，但路径由示例推断或仅部分确认；low/unconfirmed = 未能从官方页面确认，统一按 `native` 处理。
- 未使用、未请求任何 API Key，仅访问公开文档页面。未修改除本文件外的任何文件。
- 工具限制说明：WebSearch 结果质量低、基本不引用官方页面，因此全部依赖 WebFetch 直接抓取官方文档页。部分官方页面 404、重定向或内容被截断，已在条目备注中注明。

## 2. 覆盖范围

- 本次按 providers.json 共 35 个 AI 服务商（不含 `custom`、`github`、`supabase`、`cloudflare`、`generic` 等非模型服务商）。
- 其中深入调研（多模态优先清单）约 22 家，已写明每项来源或失败原因。
- 其余聚合平台（opencode-zen、opencode-go、cc-vibe、apimart、88api、grooroute、agnes、longcat、xiaomi、meta、nvidia、hunyuan、ernie 等）本次只做了有限抓取或未抓取，仅列为 `native` / unconfirmed，见第 4 节"未覆盖/未确认"。

---

## 3. 逐服务商明细

### openai — OpenAI（baseUrl `https://api.openai.com/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | openai-chat | /chat/completions | — | https://developers.openai.com/api/docs/guides/text | medium。官方页面明确 "older Chat Completions API" 仍存在，但页面未直接引用该端点 URL；路径为 OpenAI 标准路径，需复核一次。 |
| image_gen | openai-images | /images/generations | — | https://developers.openai.com/api/docs/guides/image-generation | high。页面 curl 示例：`https://api.openai.com/v1/images/generations`。 |
| tts | openai-audio-speech | /audio/speech | — | https://developers.openai.com/api/docs/guides/text-to-speech | high。页面 curl 示例：`POST https://api.openai.com/v1/audio/speech`。 |
| asr | openai-audio-transcriptions | /audio/transcriptions | — | https://developers.openai.com/api/docs/guides/speech-to-text | high。页面：`https://api.openai.com/v1/audio/transcriptions`，POST。 |
| embedding | openai-embeddings | /embeddings | — | https://developers.openai.com/api/docs/guides/embeddings | high。页面 curl 示例：`https://api.openai.com/v1/embeddings`，模型 text-embedding-3-small/large。 |
| video_gen | native | — | — | https://developers.openai.com/api/docs/guides/video-generation | high（已下线）。页面写明 Sora 2 模型与 Videos API（`POST /videos`，`GET /videos/{video_id}`）已于 **2026-09-24 下线**，不可用，因此标记 native。 |

备注：images 页面示例模型含 gpt-image-2.5 系列（页面原文）。

### gemini — Google Gemini（baseUrl `https://generativelanguage.googleapis.com/v1beta/openai`，protocol=gemini）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | openai-chat | /chat/completions | — | https://ai.google.dev/gemini-api/docs/openai | high。OpenAI 兼容基址 `https://generativelanguage.googleapis.com/v1beta/openai/`，页面声明支持 chat completions（含流式、函数调用、图像/音频输入）。 |
| embedding | openai-embeddings | /embeddings | — | https://ai.google.dev/gemini-api/docs/openai | medium。页面声明 Embeddings 受支持（示例模型 gemini-embedding-2-preview）。 |
| image_gen | openai-images | /images/generations | — | https://ai.google.dev/gemini-api/docs/openai | medium。页面声明 Images 受支持（示例 gemini-2.5-flash-image / gemini-3-pro-image-preview），但未直接引用端点路径，路径按 OpenAI 兼容约定填写。 |
| tts | native | — | — | https://ai.google.dev/gemini-api/docs/speech-generation | medium。TTS 不走 openai-audio-speech；官方示例为 Interactions API `POST https://generativelanguage.googleapis.com/v1beta/interactions`（model gemini-3.8-flash-tts）。 |
| asr | native | — | — | https://ai.google.dev/gemini-api/docs/openai | low。OpenAI 兼容页仅把音频作为 chat 输入，未见独立转写端点。 |
| video_gen | native | — | — | https://ai.google.dev/gemini-api/docs/video | low。页面称 Veo 3.1 Preview 已下线，并指向 Gemini Omni Flash（`gemini-omni-1.1-flash`）；未取得其 predict/operations 路径，无法确认。 |

备注：原生 generateContent 为 `POST https://generativelanguage.googleapis.com/v1beta/{model=models/*}:generateContent`（high，来源 https://ai.google.dev/api/generate-content）。protocol=gemini 的调用若走原生，应用 `gemini-generate` kind，但本次 invocations 以 OpenAI 兼容基址为准。

### deepseek — DeepSeek（baseUrl `https://api.deepseek.com`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | openai-chat | /chat/completions | — | https://api-docs.deepseek.com/api/create-chat-completion | high。页面给出 `POST /chat/completions`，基址 https://api.deepseek.com（页面未直接写基址，由 beta 说明推断）。 |

备注：页面未提及 TTS、ASR、embedding，不写入。图片仅作为 chat 的 image_url 输入（已在 vision 模型中体现）。

### kimi — Kimi 月之暗面（baseUrl `https://api.moonshot.cn/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | openai-chat | /chat/completions | — | https://platform.kimi.com/docs/api/chat | high。页面 `POST /v1/chat/completions`，基址 https://api.moonshot.cn。 |

备注：页面仅列 chat，未见图像、视频、TTS、ASR、embedding 接口，均不写入。

### kimi-global — Kimi Global（baseUrl `https://api.moonshot.ai/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | native | — | — | https://platform.kimi.com/docs/api/chat | low。文档基址为 api.moonshot.cn，providers.json 为 api.moonshot.ai，主机不同，按规则标 native。若确认 .ai 同路径可改为 openai-chat。 |

### minimax-global — MiniMax EN（baseUrl `https://api.minimax.io/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | native | — | — | https://platform.minimax.io/docs/api-reference/text-chat-openai.md | low。官方页面确认 OpenAI SDK 基址 `https://api.minimax.io/v1`，但未直接写出 `/chat/completions` 路径，故不填。 |
| image_gen | native | /image_generation | — | https://platform.minimax.io/docs/api-reference/image-generation-t2i.md | high（路径）。`POST https://api.minimax.io/v1/image_generation`，请求体为 MiniMax 自有格式，非 openai-images，故 native，路径仅供参考。 |
| tts | native | /t2a_v2 | — | https://platform.minimax.io/docs/api-reference/speech-t2a-http.md | high（路径）。`POST https://api.minimax.io/v1/t2a_v2`，自有请求体格式，故 native。 |
| video_gen | native | — | — | https://platform.minimax.io/docs/api-reference/video-generation-v2-create.md | high（路径）。提交为 `POST https://api.minimax.io/v2/video_generation`，位于 `/v2` 而非 `/v1` 基址下，相对路径需 `..`，违规，故 native；轮询路径未取得。 |

### minimax — MiniMax CN（baseUrl `https://api.minimaxi.com/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| 全部 | native | — | — | https://platform.minimax.io/docs/llms.txt | low。国内域 api.minimaxi.com 未直接抓取官方页面；同 minimax-global 的接口形态，但主机不同，按规则 native。需单独复核国内域。 |

### glm — GLM 智谱（baseUrl `https://open.bigmodel.cn/api/coding/paas/v4`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | native | — | — | https://docs.z.ai/api-reference/llm/chat-completion.md | medium。通用 chat 端点为 `POST https://api.z.ai/api/paas/v4/chat/completions`；providers.json 为 `/api/coding/paas/v4`（Coding Plan 专属端点），base 路径不同，按规则标 native。国内域 open.bigmodel.cn 未单独抓取。 |
| image_gen / video_gen / asr | native | — | — | https://docs.z.ai/api-reference/image/generate-image.md | medium。通用端点：images `/paas/v4/images/generations`；视频提交 `/paas/v4/videos/generations`（查询路径未取得）；ASR `/paas/v4/audio/transcriptions`（model glm-asr-2512）。均位于通用 base，与 coding base 不一致，标 native。 |

### zai — Z.ai（baseUrl `https://api.z.ai/api/coding/paas/v4`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat / image_gen / video_gen / asr | native | — | — | https://docs.z.ai/api-reference/introduction | medium。与 glm 相同：官方通用基址 `https://api.z.ai/api/paas/v4`，coding 端点不同，全部标 native。通用路径（供人工核对）：chat `/chat/completions`，images `/images/generations`，videos `/videos/generations`，ASR `/audio/transcriptions`。 |

### qwen — 通义千问 DashScope（baseUrl `https://dashscope.aliyuncs.com/compatible-mode/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | openai-chat | /chat/completions | — | https://help.aliyun.com/zh/model-studio/compatibility-of-openai-with-dashscope | medium。页面给出兼容端点 `.../compatible-mode/v1/chat/completions`（工作空间域名形式），并称旧域 dashscope.aliyuncs.com 仍可用。 |
| embedding | openai-embeddings | /embeddings | — | https://help.aliyun.com/zh/model-studio/embedding-interfaces-compatible-with-openai | medium。页面给出 `POST .../compatible-mode/v1/embeddings`（工作空间域名），同一基础路径。 |
| image_gen / tts / asr | native | — | — | https://help.aliyun.com/zh/model-studio/text-to-speech | medium。兼容模式页面未提供图像生成端点；页面称 Qwen-Audio 不支持 OpenAI 兼容协议，仅支持 DashScope 协议（TTS 为 WebSocket，一次性 HTTP 见非实时 TTS 指南）。 |

### qianwen-platform — 千问AI平台（baseUrl `https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | native | — | — | https://help.aliyun.com/zh/model-studio/compatibility-of-openai-with-dashscope | low。官方兼容页的工作空间形式为 `https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/compatible-mode/v1`；token-plan.* 主机与之不同，未能确认，按规则标 native。 |
| tts | native | — | — | 同上 | low。`qwen-audio-3.0-tts-plus` 不支持 OpenAI 兼容协议，只能走 DashScope 协议。 |

### volcengine — 火山引擎 / 豆包（baseUrl `https://ark.cn-beijing.volces.com/api/coding/v3`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat / image_gen / video_gen | native | — | — | https://docs.volcengine.com/docs/82379/1361424 | low。抓取到的官方页面为"管理 API Key"页，未取得 Ark 接口路径；providers.json 的 `/api/coding/v3` 为 Coding Plan 端点，与通用 `/api/v3` 不同，全部标 native。通用 `/api/v3` 下的标准路径未经官方页面确认前不填。 |

### byteplus — BytePlus（baseUrl `https://ark.ap-southeast.bytepluses.com/api/coding/v3`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| 全部 | native | — | — | — | low。未抓取官方页面；Coding Plan 端点（`/api/coding/v3`），按规则标 native。 |

### anthropic — Anthropic（baseUrl `https://api.anthropic.com`，protocol=anthropic）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | anthropic-messages | /v1/messages | — | https://platform.claude.com/docs/en/api/messages | high。页面 `POST /v1/messages`，基址 https://api.anthropic.com。 |

### xai — xAI Grok（baseUrl `https://api.x.ai/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| image_gen | openai-images | /images/generations | — | https://docs.x.ai/developers/rest-api-reference/inference/images | medium。页面 `POST /v1/images/generations`，基址 https://api.x.ai。另有 `POST /v1/images/edits`、`GET /v1/images/{request_id}`（轮询，仅 deferred 请求）。 |
| chat | native | — | — | https://docs.x.ai/docs/api-reference | low。抓取到的页面只有 Responses API（`POST /v1/responses`），未直接确认 chat/completions，故 native。 |
| video_gen | native | — | — | — | low。未见视频端点。 |

### hunyuan — 腾讯混元 TokenHub（baseUrl `https://api.lkeap.cloud.tencent.com/plan/v3`，kind=aggregator）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | native | — | — | https://cloud.tencent.com/document/product/1729/101848 | medium。混元官方 chat 为腾讯云 API 动作 `ChatCompletions`（文档 /document/api/1729/105701），是云 API 签名风格，非 OpenAI 路径；按规则 native。 |
| image_gen | native | — | — | 同上 | medium。官方为异步作业 `SubmitHunyuanImageJob` / `QueryHunyuanImageJob`（云 API），非标准路径；native。 |

### ernie — ERNIE 百度千帆（baseUrl `https://qianfan.baidubce.com/v2/coding`，kind=aggregator）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | native | — | — | https://cloud.baidu.com/doc/qianfan-api/s/3m7of64lb | medium。通用 chat 为 `POST https://qianfan.baidubce.com/v2/chat/completions`；providers.json 为 `/v2/coding`（Coding Plan），base 路径不同，标 native。官方页未自称 OpenAI 兼容。 |
| image_gen / embedding / tts | native | — | — | 同上 | low。页面列出接口名（图像生成、向量、语音合成）但未给出路径。 |

### stepfun — Stepfun 阶跃星辰（baseUrl `https://api.stepfun.com/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | openai-chat | /chat/completions | — | https://platform.stepfun.com/docs/api-reference/chat/chat-completion-create | high。页面 `POST https://api.stepfun.com/v1/chat/completions`。 |

备注：Step Plan 通道为 `https://api.stepfun.com/step_plan/v1/chat/completions`，providers.json 未使用，不写入。其它能力页面未给出路径，标 native/未确认。

### mistral — Mistral AI（baseUrl `https://api.mistral.ai/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | openai-chat | /chat/completions | — | https://docs.mistral.ai/api/endpoint/chat | high。页面 `POST /v1/chat/completions`。 |
| embedding | openai-embeddings | /embeddings | — | https://docs.mistral.ai/api/endpoint/embeddings | high。页面 `POST /v1/embeddings`，模型 mistral-embed。 |
| asr / tts | native | — | — | https://docs.mistral.ai/api/endpoint/chat（导航链接） | low。页面仅给出 `/api/endpoint/audio/transcriptions`、`/api/endpoint/audio/speech` 的文档链接，未给出 API 路径，未确认，标 native。 |

### cohere — Cohere（baseUrl `https://api.cohere.ai/compatibility/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat / embedding | native | — | — | https://docs.cohere.com/reference/chat | medium。原生 chat 为 `POST https://api.cohere.com/v2/chat`，embedding 为 `/v2/embed`；providers.json 的兼容端点 `api.cohere.ai/compatibility/v1` 未在本次抓取的官方页面中确认，主机与 base 均不同，按规则 native。 |

### groq — Groq（baseUrl `https://api.groq.com/openai/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | openai-chat | /chat/completions | — | https://console.groq.com/docs/api-reference | high。页面 `POST https://api.groq.com/openai/v1/chat/completions`。 |
| asr | openai-audio-transcriptions | /audio/transcriptions | — | 同上 | high。页面 `POST https://api.groq.com/openai/v1/audio/transcriptions`。 |
| tts | openai-audio-speech | /audio/speech | — | 同上 | high。页面 `POST https://api.groq.com/openai/v1/audio/speech`。 |

备注：页面未列 embeddings、image_gen；不写入。

### together — Together AI（baseUrl `https://api.together.xyz/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | native | — | — | https://docs.together.ai/reference/chat-completions-1 | medium。官方页面路径为 `POST /v1/chat/completions`，但 server 基址是 `https://api.together.ai`，providers.json 为 `api.together.xyz`，主机不同，按规则 native。若确认 xyz 为同一 API 别名，可改为 openai-chat。其余能力页面未覆盖。 |

### nvidia — NVIDIA NIM（baseUrl `https://integrate.api.nvidia.com/v1`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat / embedding / image_gen | native | — | — | https://docs.nvidia.com/nim/large-language-models/latest/api-reference.html | low。抓取结果为空，未确认任何路径。 |

### perplexity — Perplexity（baseUrl `https://api.perplexity.ai`）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | native | /v1/sonar | — | https://docs.perplexity.ai/api-reference/chat-completions-post | high（路径）。官方为 `POST https://api.perplexity.ai/v1/sonar`，非 OpenAI 形态，故 native。页面未列其它能力。 |

### openrouter — OpenRouter（baseUrl `https://openrouter.ai/api/v1`，kind=aggregator）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| chat | openai-chat | /chat/completions | — | https://openrouter.ai/docs/api/reference/overview | high。页面 `POST /api/v1/chat/completions`，基址 https://openrouter.ai/api/v1。 |
| embedding / image_gen / video_gen / tts / asr | native | — | — | https://openrouter.ai/openapi.json | low。页面未覆盖；OpenAPI 抓取仅得到 components 段（被截断），path 列表未获取，不填。 |

### xiaomi、longcat、meta、agnes、opencode-zen、opencode-go、cc-vibe、apimart、88api、grooroute（未覆盖）

| capability | kind | path | pollPath | official source URL | notes (confidence) |
|---|---|---|---|---|---|
| 全部 | native | — | — | — | low。本次未抓取这些服务商的官方文档（均为聚合/中转或本次时间内未覆盖），全部标 native / unconfirmed，待后续专项调研。 |

---

## 4. 未覆盖 / 未确认（汇总）

- 未覆盖：xiaomi、longcat、meta、agnes、opencode-zen、opencode-go、cc-vibe、apimart、88api、grooroute、minimax（CN 域）、kimi-global 的官方页面。
- 抓取失败或内容不足：
  - Volcengine（docs.volcengine.com 重定向后返回"管理 API Key"页，未获取 Ark 接口路径）。
  - NVIDIA（页面为空）。
  - OpenRouter OpenAPI（只拿到 components，未取得 paths）。
  - OpenAI 的 chat 路径（chat reference 页面 404，改用 guide 页面间接确认）。
  - MiniMax 视频查询路径、Gemini Veo 路径、Z.ai 视频查询路径。
- 搜索工具：WebSearch 返回的结果基本是第三方站，未采用。

## 5. 结论汇总

| provider id | 已确认非 native 的能力（high/medium） |
|---|---|
| openai | chat(medium), image_gen, tts, asr, embedding（video_gen 已下线，native） |
| gemini | chat, embedding(medium), image_gen(medium)；tts/asr/video_gen native |
| deepseek | chat |
| kimi | chat |
| mistral | chat, embedding；asr/tts native |
| groq | chat, asr, tts |
| openrouter | chat |
| stepfun | chat |
| anthropic | chat（anthropic-messages） |
| xai | image_gen(medium)；chat native |
| qwen | chat(medium), embedding(medium)；image_gen/tts/asr native |

统计：35 个 AI 服务商中，**11 家**至少有 1 项已确认非 native；合计 **21 条**非 native 能力（high 或 medium）。其余 24 家以 native 为主或未覆盖。

- 确认为 native（即没有可写入的标准 invocations）的主要原因：Coding Plan / token-plan / 云 API 风格端点与通用 base 不一致（glm、zai、volcengine、byteplus、ernie、qianwen-platform、hunyuan、cohere、minimax、minimax-global 的多数能力）；非 OpenAI 形态的请求体（minimax image/tts、perplexity sonar、gemini TTS 的 Interactions API）。
- 失效能力：OpenAI Videos API（Sora）已于 2026-09-24 下线；Gemini Veo 3.1 Preview 已下线。
- 异步视频/图片：本次没有任何一条能完整确认提交路径+轮询路径，因此 `async-task` 一条都未写入。

## 6. 后续建议（给 M1b / 复核）

1. 将 `openai`、`groq`、`mistral`、`openrouter`、`stepfun`、`anthropic`、`qwen`（chat/embedding）的 high/medium 条目作为首批写入候选；写入前复核 openai chat 与 gemini image 两条 medium。
2. 对 Volcengine、NVIDIA、OpenRouter 其它能力、MiniMax CN、kimi-global、together 主机别名，需要单独再抓取官方页面。
3. Coding Plan 类 baseUrl（glm、zai、volcengine、byteplus、ernie、qianwen-platform）的能力全部为 native，如需 native 之外的调用方式，需要人工确认是否为通用端点（不同 base 路径），再决定是否在 providers.json 之外补充。
