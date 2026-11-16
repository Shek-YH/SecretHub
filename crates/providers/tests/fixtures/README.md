# Model-list fixtures

Hand-trimmed, contains no real keys or account data. Never fetched with credentials.

| File | Source | Verified fields | Assumed |
|---|---|---|---|
| `openrouter.json` | Real response of the public unauthenticated `https://openrouter.ai/api/v1/models` (fetched 2026-10-16), trimmed to 6 models and the fields `id`, `name`, `architecture.{modality,input_modalities,output_modalities}`, `supported_parameters` | all | none |
| `openai.json` | Hand-written; shape `data[].id/object/created/owned_by` is the documented OpenAI `GET /v1/models` format | `data[].id` | model IDs are examples |
| `anthropic.json` | https://platform.claude.com/docs/en/api/models-list | `data[].id`, `display_name`, `capabilities.image_input.supported`, `capabilities.thinking.supported`, `has_more`, `last_id`, `first_id` | model IDs are examples |
| `gemini.json` | https://ai.google.dev/api/models (models.list) | `models[].name` (`models/` prefix), `displayName`, `supportedGenerationMethods`, `thinking`, `nextPageToken` | the value `embedContent` is not enumerated on that page; it is the method name Gemini embedding models report |
| `newapi.json` | https://github.com/QuantumNous/new-api `relaykit/dto/pricing.go` (JSON key `supported_endpoint_types` on OpenAI model objects) and `relaykit/types/endpoint_type.go` (values: openai, openai-response, openai-response-compact, openai-alpha-search, anthropic, gemini, jina-rerank, image-generation, embeddings, openai-video) | key name and value strings | model IDs are examples |
