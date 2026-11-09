---
name: secrethub-import
description: >-
  Use this skill when the user wants servers, databases, cloud-service logins,
  API keys or other credentials from files or folders on their machine brought
  into SecretHub: "把 F:\资料\服务器 里的配置导入 SecretHub", "导入这个文件夹",
  "把这些 .env 和密钥文件整理进密码库". You locate the paths by directory and
  file names only and hand the absolute paths to SecretHub Desktop, which reads
  the files locally, groups them, removes duplicates and shows the user a review
  of names and choices. You never read the files, so their contents never enter
  the AI conversation.
compatibility: Requires SecretHub MCP tools connected to the local SecretHub Desktop vault, with Desktop open and unlocked.
---

# SecretHub import

Bring local configuration files into SecretHub without the AI ever seeing what
is inside them. Desktop reads, parses and de-duplicates the files in its own
process; the user reviews names and choices (never values) and confirms. You
only pass paths, remind the user to review, and report the outcome.

## Non-negotiable prohibitions

- **Never read the contents of the files.** Do not Read, cat, head, grep,
  open in an editor or otherwise display `.env` files, private keys (`id_rsa`,
  `id_ed25519`, `*.pem`), `.json` configs, `.csv` password exports, `.txt`
  notes or any other file that may hold credentials. Searching *inside* files
  counts as reading them.
- **Never** ask the user to paste file contents, passwords or keys into chat.
- Use only **directory listings and file names** to find what to import (for
  example `ls` / `Glob` on the folder). If a name is ambiguous, ask the user
  which folder they mean instead of opening files to find out.
- Only pass paths the user named or clearly pointed at. Do not widen the
  request to parent folders, the home directory or other drives.

## Workflow

### 1. Find the paths by name only

Ask the user for the folder(s) or file(s) if they did not give them. List
directory entries to confirm the paths exist and look right (names such as
`服务器A`, `.env`, `id_ed25519`, `users.csv`). Collect **absolute** paths, a
folder path covers its sub-folders (three levels deep, 200 files at most).

### 2. Queue the request: `secrethub_request_import`

Call it with `paths` (1 to 20 absolute file or folder paths). It only queues
the paths and returns a `requestId`; nothing is read or written yet. If it
returns an error (relative path, too many paths), fix the paths with the user.

### 3. Ask the user to review in Desktop

Tell the user to open SecretHub Desktop, find the banner "AI 请求导入", click
"查看", check the list (config groups, split-off API keys, duplicates against
what is already stored, per-item choices) and confirm. Explain that they can
uncheck items and choose between merge, keep both and skip for duplicates, and
that no values are shown in the review.

### 4. Poll the outcome: `secrethub_get_import_status`

Call it with the `requestId`:

- `pending`: the user has not reviewed yet; remind them, do not re-queue.
- `done`: report `added`, `updated` and `skipped` counts and the item names.
  Names only; there are no values in the result and you must not ask for them.
- `declined`: the user rejected it; ask what to change, do not re-queue the
  same paths.
- `failed`: report the error code only and ask the user to check Desktop.
- `not_found`: wrong or expired id; confirm it or queue the request again.

### 5. Follow-ups

New API keys are checked automatically by Desktop after the import. If the
user wants model lists synced, use `secrethub_request_model_sync`. To use an
imported config group in a project, follow the `secrethub-project-env` skill
(bind the group, or a subset of its variables, in a plan).

## Failure handling

- If SecretHub MCP is unavailable, say so; do not fall back to reading the
  files yourself or asking the user to paste anything.
- If the user insists you open a file "just to check", refuse and explain that
  Desktop does the reading so the contents stay on their machine.

## Completion checklist

- No file contents were read, printed or summarized.
- Paths were absolute and limited to what the user named.
- The user was told to review the request in Desktop.
- The final report contains counts and item names only.
