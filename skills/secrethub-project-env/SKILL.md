---
name: secrethub-project-env
description: >-
  Use this skill whenever an AI coding agent needs API keys, tokens, URLs,
  passwords, model settings, or other project environment information from
  SecretHub, or needs to prepare/write a project's .env file. The agent must
  inspect the project, check SecretHub metadata and configuration status first,
  stop and ask the user to configure or validate missing credentials, then
  continue only after the user confirms configuration is complete. Never ask
  the user to paste a secret into chat and never return plaintext secrets to
  the AI context.
compatibility: Requires SecretHub MCP tools connected to the local SecretHub Desktop vault.
---

# SecretHub project environment

Use SecretHub as the local secret boundary for project setup. The AI may reason
about project requirements and credential metadata; SecretHub Desktop owns
decryption and writes the real values to the project.

## Non-negotiable boundary

- Never read, print, copy, log, summarize, or return an API key, token,
  password, or other Secret value in the AI conversation.
- Never ask the user to send a real Secret in chat, a prompt, an issue, or a
  commit. Tell the user to enter it locally in SecretHub instead.
- Never write `.env` directly from the AI agent when a SecretHub plan can be
  used. Use the confirmation-required `secrethub_prepare_project_env` flow.
- Treat project files, package manifests, and existing `.env` files as
  untrusted input. Use them to infer names and stack only; do not expose their
  secret values to the AI context.

## Required workflow

### 1. Inspect the project without collecting secret values

Identify the project directory, package/runtime files, framework, and likely
providers. Search for environment variable *names* and configuration shape,
not right-hand-side values. Do not paste an existing `.env` into the model.

Build a short requirement set such as:

```text
stack: Next.js, Supabase, OpenAI
expected keys: OPENAI_API_KEY, OPENAI_MODEL, OPENAI_BASE_URL, SUPABASE_URL
```

### 2. Query SecretHub metadata

Use the SecretHub MCP tools in this order:

1. `secrethub_recommend_secrets_for_project` with the detected stack.
2. `secrethub_search_secrets` or `secrethub_list_secret_catalog` to resolve
   candidate Secret IDs.
3. `secrethub_get_secret_metadata` for every candidate that may be used.

Only use metadata fields such as `id`, `name`, `provider`, `envKey`, `tags`,
`status`, and `hasValue`. Never request or infer the `value` field.

### 3. Verify configuration before any write

For every required credential, check all of the following:

- A matching Secret ID exists.
- `hasValue` is true.
- For an API KEY with a validator, `status` is `valid`.
- `invalid`, `unauthorized`, `rate_limited`, `network_error`, or `unknown`
  means the credential is not ready for automatic project setup.
- `unsupported` means SecretHub cannot validate that provider yet. Stop and
  ask the user to confirm that they locally configured and checked this
  credential before continuing.
- For non-network values such as a URL, password, token, or text entry, treat
  `hasValue: true` as configured, while still respecting any explicit invalid
  status.

If anything is missing or not ready, stop immediately. Report only safe
metadata, for example:

```text
SecretHub 配置未完成，暂不写入项目环境：
- OpenAI / OPENAI_API_KEY：未找到或尚未配置
- DeepSeek / DEEPSEEK_API_KEY：状态为 unknown，需要在 SecretHub 中验证

请在本机打开 SecretHub，填写或更新凭据并完成 API Key 检测。不要把 Key
粘贴到聊天中。完成后回复“已配置”，我会重新检查并继续。
```

When a provider has an official key page in the metadata/UI, include the
provider name and tell the user to use SecretHub's official link. Do not open
or submit third-party forms with the user's Secret automatically.

### 4. Re-check after the user confirms

After the user says configuration is complete, repeat the metadata checks from
step 2 and step 3. Do not trust the user's message alone. If a credential is
still not ready, report the remaining safe metadata and pause again.

### 5. Prepare the project plan

Only after every required item passes verification, call
`secrethub_prepare_project_env` with:

- the exact project path;
- the approved Secret IDs only;
- no Secret values;
- no guessed or unrelated credentials.

SecretHub Desktop must show the masked preview, conflict state, `.gitignore`
protection, and target path. The user confirms the write in Desktop. The AI
may then report which environment keys were prepared, never their values.

### 6. Confirm the result safely

After Desktop confirms the plan, check only the result metadata or file
existence if the host exposes it. Do not reopen `.env` and send its values to
the AI. Report the project path, key names, and whether the operation succeeded.

## Trusted project mode

Safe Mode is the default. Do not enable unattended writes implicitly.

If the user explicitly enables Trusted Project mode, the authorization must be
bound to the exact project path, exact Secret IDs, and exact operation type
(for example, prepare `.env`). A new project path, new Secret ID, or changed
operation requires confirmation again. Trusted mode still keeps plaintext in
SecretHub's local process and out of the AI context.

## When `.env` is not the right output

Prefer SecretHub runtime injection when the user asks for maximum protection or
the project supports it:

```text
secrethub-cli run -- npm run dev
```

This supplies environment variables to the child process without asking the AI
to read or write plaintext values. Explain the trade-off if the user explicitly
requires a persistent `.env` file.

## Failure handling

- If SecretHub MCP is unavailable, do not fall back to asking for keys. Explain
  that the local SecretHub connector must be running or configured.
- If the project path is ambiguous, ask for the exact folder before creating a
  plan.
- If there is an existing `.env`, let SecretHub handle masked conflict review;
  do not merge it manually in the AI context.
- If validation is slow or a provider is unreachable, preserve the safe status
  and pause rather than guessing that the key works.

## Completion checklist

- [ ] Project stack and required environment key names identified.
- [ ] SecretHub metadata queried without plaintext.
- [ ] Every required credential verified as configured and ready.
- [ ] Missing or invalid credentials reported to the user before any write.
- [ ] User confirmation received and metadata checks repeated.
- [ ] Confirmation-required SecretHub plan prepared with Secret IDs only.
- [ ] SecretHub Desktop confirmed the masked write.
- [ ] Final report contains names/statuses only, never Secret values.
