---
name: secrethub-secure-input
description: >-
  Use this skill whenever you need a password, API key, token, account or any
  other sensitive value from the user: "把后台账号存进 SecretHub", "我给你数据库
  密码", "这个服务要 Key", or any step where you would otherwise ask the user to
  paste a secret into chat. You queue a form tailored to the case with
  secrethub_request_input; SecretHub Desktop shows it, the user fills it in,
  Desktop verifies it (login endpoint or the provider's /models) and stores it
  in the vault. You only get a status and the new item id, never the value.
compatibility: Requires SecretHub MCP tools connected to the local SecretHub Desktop vault, with Desktop open and unlocked.
---

# SecretHub secure input

Collect a sensitive value without it ever entering the conversation. You design
the form; the user types into SecretHub Desktop; Desktop verifies and stores.

## Non-negotiable prohibitions

- **Never** ask the user to paste, type, send, screenshot or read out a
  password, key or token in chat, a file or any prompt. If they offer one,
  refuse and queue a form instead.
- **Never** prefill a `password` field (the tool rejects it) and never put a
  secret into `value`, `hint`, `label` or `name`.
- **Never** read the form with screenshots, UI Automation or accessibility
  APIs, and never type into it for the user.
- Only point `verify.url` / the base URL at the service the user named. Desktop
  shows the user where the values will be sent.

## Which tool

| Situation | Tool |
|---|---|
| A website / admin panel account, a database, SSH, object storage, any custom credential | `secrethub_request_input` |
| An LLM key where you want the user to pick a model, or a provider whose base URL you have to set, or verification before saving | `secrethub_request_input` (`item_type: "api_key"`, `verify.type: "llm"`) |
| An LLM provider in the catalog with an official key page, and the user is about to create a brand-new key on that site | `secrethub_start_key_capture` (clipboard capture, see `secrethub-key-onboarding`) |
| Importing credentials that already sit in files on this machine | `secrethub_import_inspect` (see `secrethub-import`) |

## Building the form

`secrethub_request_input` arguments:

- `title` (what the user sees), `hint` (one or two sentences of guidance),
  `name` (name of the vault item), `item_type`: `login` | `api_key` | `env_group`.
- `fields`: `{ key, label, type: text|password|select, value?, options?, required?, role? }`.
- `verify`: `{ type: "none" }` | `{ type: "login", url, body_user?, body_pass?, ok_field? }` | `{ type: "llm", auth: "bearer" | "x-api-key" }`.

Per item type:

- **login** (website / admin account, kept for the user only; AI tools never
  see login items, so use `env_group` when a project needs the values in `.env`): field keys only `website`, `username`,
  `password`, `totp`, `notes`; `username` is required. To verify, give
  `verify.type: "login"` with the login endpoint (`https`); Desktop POSTs JSON
  `{ <body_user>: username, <body_pass>: password }` and accepts 2xx where
  `ok_field` (if given) is truthy. Find the endpoint and body field names in the
  project's code or API docs first; if unsure, use `verify.type: "none"`.
- **api_key** (LLM provider): `provider_id` is required (catalog id). Fields
  are env variable names and each has a role: exactly one `role: "api_key"`
  (`type: password`), optional `role: "base_url"` (text) and `role: "model"`
  (select or text). Leave the base URL `value` empty to have Desktop prefill it
  from the provider catalog; set it only when the user uses a different
  gateway, after checking the official docs. Use `verify.type: "llm"` with
  `auth: "bearer"` for OpenAI-compatible APIs and `"x-api-key"` for Anthropic;
  the form then has a **获取模型列表** button that fills the model dropdown.
- **env_group** (anything else: database, SSH, storage, webhook secrets):
  fields are env variable names (`A-Z`, digits, `_`); `password` fields are
  stored as secrets, `text` fields as plain values.

Prefill everything you already know (username, host, base URL, model) so the
user only types what only they know.

## Workflow

1. Work out which service, which values are needed and where they are verified.
2. Call `secrethub_request_input`. Fix the template if it is rejected (the error
   names the rule).
3. Tell the user: open SecretHub Desktop (unlock it if asked), fill in the form
   titled `<title>` and click **验证并保存**. Failed verification keeps the
   form open with the reason; they can correct and retry or cancel.
4. Poll `secrethub_get_input_status({ request_id })` every few seconds:
   `waiting` → keep waiting (stop after ~10 minutes and ask);
   `saved` → you get `itemId` and `itemType`; `cancelled` → stop and ask;
   `failed` → `error.code` (`invalid_template`, `expired`): fix and re-queue.
5. Use the item by id afterwards: `secrethub_get_secret_metadata`,
   `secrethub_prepare_project_env`, `secrethub_request_model_sync` (api_key).
   Personal `login` items are not visible to AI tools at all.
6. Report the item name, type and verification outcome. Never a value.

## Checklist

- [ ] No secret requested in chat; no password field prefilled.
- [ ] Verification target is the service the user named (https).
- [ ] Outcome read from `secrethub_get_input_status` only.
