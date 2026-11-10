---
name: secrethub-import
description: >-
  Use this skill when the user wants servers, databases, cloud-service logins,
  API keys or other credentials from files or folders on their machine brought
  into SecretHub: "把 F:\资料\服务器 里的配置导入 SecretHub", "导入这个文件夹",
  "把这些 .env 和密钥文件整理进密码库". You locate the paths by directory and
  file names only, ask SecretHub Desktop for a masked view of the files (every
  secret is a placeholder), organize that view into config groups, API keys and
  notes, and submit the plan; Desktop writes it automatically under guard rails
  and the user can undo it. You never read the files, so their contents never
  enter the AI conversation. Also use it when a project call or connection
  fails because stored information looks wrong.
compatibility: Requires SecretHub MCP tools connected to the local SecretHub Desktop; Desktop must be open, and unlocked for the final write.
---

# SecretHub import

Bring local configuration files into SecretHub without the AI ever seeing a
secret. Desktop reads the files in its own process and shows you a **masked
view**: names, labels, hosts, ports, short user names and URLs stay readable;
passwords, tokens, keys and anything it is unsure about are placeholders such
as `«S3:24 位随机串»`. You organize that view into a plan that refers to the
placeholders. Desktop swaps in the real values itself, writes the plan without
asking, never changes or deletes what the vault already holds, and keeps an
undo for the whole batch.

## Non-negotiable prohibitions

- **Never read the contents of the files.** Do not Read, cat, head, grep,
  open in an editor or otherwise display `.env` files, private keys (`id_rsa`,
  `id_ed25519`, `*.pem`), `.json` configs, `.csv` password exports, `.txt`
  notes or any other file that may hold credentials. Searching *inside* files
  counts as reading them. The masked view from Desktop is the only way you see
  their structure.
- **Never** ask the user to paste file contents, passwords or keys into chat.
- Use only **directory listings and file names** to find what to import (for
  example `ls` / `Glob` on the folder). If a name is ambiguous, ask the user
  which folder they mean instead of opening files to find out.
- Only pass paths the user named or clearly pointed at. Do not widen the
  request to parent folders, the home directory or other drives.
- A plan may contain only placeholder numbers (`{ "token": "S3" }`) and text
  that is visible in the masked view (`{ "literal": "8.0.0.1" }`). Never guess,
  invent or "complete" a password, key or token. If a value is not in the view,
  leave the variable out and tell the user what is missing.
- Descriptions and notes explain purpose only. They must never contain a value.

## Default workflow

### 1. Find the paths by name only

Ask for the folder(s) or file(s) if they were not given. List directory
entries to confirm the paths exist (names such as `服务器A`, `.env`,
`id_ed25519`, `users.csv`). Collect **absolute** paths; a folder covers its
sub-folders (three levels, 200 files at most).

### 2. Get the masked view: `secrethub_import_inspect`

Call it with `paths` (1 to 20 absolute paths). It returns a `requestId`, which
is also the `inspection_id`. Desktop scans locally; the vault does not need to
be unlocked for this step. Poll `secrethub_get_import_inspection` with the
`requestId`:

- `pending`: Desktop has not answered yet; wait, and remind the user to keep
  SecretHub Desktop running.
- `done`: `files` lists every line of every file with placeholders instead of
  secrets. Large views are paged; pass `nextOffset` as `offset` until it is
  null.
- `expired`: the placeholders lived 30 minutes; inspect again.
- `failed` or `not_found`: report the code and check the paths with the user.

Equal values share one placeholder number, so a repeated number is a repeated
value (a private key in the `.env` and in a key file, for example).

### 3. Organize the view

Build one plan from what you read:

- **One group per server or service** (`type: "env_group"`, with a `category`
  of `server`, `database`, `storage` or `other`). A provider API key that
  belongs to a known service can be `type: "api_key"` with its `providerId`
  and the key variable; use `login` for website accounts (variables named
  `website`, `username`, `password`) and `note` for free text worth keeping.
- **Variable names**: UPPER_CASE, with a prefix taken from the group (for
  example `ALI_BT_SSH_HOST`), so that several groups never collide when they
  are exported into one `.env`.
- **Recognize common structures**: BT (宝塔) panel (URL, user, password, API
  token, IP whitelist), SSH (host, port, user, password, private key), database
  (host, port, user, password, name), object storage (endpoint, region, bucket,
  access key id/secret).
- **Describe every variable** in one short sentence of purpose
  (`"SSH 登录密码"`, `"宝塔面板 API 接口密钥"`). No values in descriptions.
- **Flag doubts** for the final report: duplicates, a value that looks like it
  belongs elsewhere, a password next to the wrong host, a half-filled group.
  Do not silently fix or drop them; use what the view shows and mention them.

### 4. Submit the plan: `secrethub_apply_import_plan`

Call it with `inspection_id` and `items`. Values are `{ "token": "S3" }` for
placeholders and `{ "literal": "..." }` only for text that appears in the view
(a host and a port may be joined into a URL). A value that came from a
placeholder is always stored as a secret, whatever `secret` says. Desktop
writes the plan as soon as the vault is unlocked, with no dialog:

- a group with the same name only receives the variables it lacks; the same
  name with a different value goes to a new group called `<name>（AI 导入
  <date>）`; nothing existing is overwritten or deleted;
- an API key or login with the same value is skipped, a different one is kept
  as an additional entry;
- everything is recorded as one batch that the user can undo in Desktop.

### 5. Poll and report: `secrethub_get_import_status`

Use the plan's `requestId`:

- `pending`: the vault is locked or Desktop is closed; ask the user to unlock
  SecretHub, the write then happens by itself.
- `done`: per item `created`, `added_vars`, `kept_both`, `skipped` or
  `rejected`, plus `rejected` variables with a reason (never a value) and the
  `batchId`. Fix rejected variables with a corrected plan only when the reason
  is something you can correct (`invalid_name`, `unknown_token`,
  `literal_not_in_view`, `description_has_value`).
- `failed`: report the code (`inspection_expired` means inspect again).

Then tell the user briefly: what was added (group and entry names, counts),
which doubts you noticed, and that SecretHub Desktop shows a notice with
"查看" and "撤销此次导入" for this batch. Names and counts only; there are no
values in any result and you must not ask for them.

## When a project later fails to connect

If a project call, SSH login or API request fails with a credential or
connection error for something that came from an import, do not read the
source files and do not try to repair the value yourself. Tell the user which
information may be wrong ("阿里宝塔服务器的 SSH 密码可能不对"), ask them to
update it in SecretHub or to point you at the corrected file, then repeat the
workflow above for it. Use `secrethub_request_import` instead only when the
user explicitly prefers to review every item themselves in Desktop (a manual
review dialog with merge, keep-both and skip choices); poll
`secrethub_get_import_status` for it as well.

## Follow-ups

New API keys are checked automatically by Desktop after an import. If the
user wants model lists synced, use `secrethub_request_model_sync`. To use an
imported config group in a project, follow the `secrethub-project-env` skill
(bind the group, or a subset of its variables, in a plan).

## Failure handling

- If SecretHub MCP is unavailable, say so; do not fall back to reading the
  files yourself or asking the user to paste anything.
- If the user insists you open a file "just to check", refuse and explain that
  Desktop does the reading and masks every secret, so the contents stay on
  their machine.

## Completion checklist

- No file contents were read, printed or summarized.
- Paths were absolute and limited to what the user named.
- The plan used only placeholders and text visible in the masked view.
- Every variable has a purpose description without a value.
- The final report has names, counts and doubts only, and mentions the undo.
