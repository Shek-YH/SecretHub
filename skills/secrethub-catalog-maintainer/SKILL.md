---
name: secrethub-catalog-maintainer
description: >-
  Use this skill when the SecretHub maintainer wants to refresh the bundled or
  remote model catalog (packages/provider-definitions/model-catalog.json):
  check each provider's public documentation for new, retired or renamed
  models, default-model changes and capability tags, propose a change table
  with source links, and after the maintainer approves it edit the catalog,
  run catalog:check, and guide the sign, upload and bake steps. Public sources
  only; never needs or handles an API key or the signing private key.
compatibility: Works inside the SecretHub repository; needs Node 22+ and the npm scripts catalog:check, catalog:sign, catalog:bake.
---

# SecretHub model catalog maintainer

The catalog tells SecretHub which models to suggest per provider. It never
contains URLs or env names; those stay in `providers.json` and ship with app
releases. A signed copy of `model-catalog.json` is downloaded by installed apps,
so a bad edit reaches users quickly. Work carefully and keep the maintainer in
control of every change.

## Ground rules

- Use public sources only: the provider's official docs, model list pages,
  changelogs or release notes. Do not use API keys, the user's vault, or the
  SecretHub MCP tools for this task.
- Every claim in the change table needs a source link. If a fact cannot be
  confirmed on an official page, mark it "unverified" and leave the catalog
  unchanged for it.
- Never edit `model-catalog.json` before the maintainer approves the table.
- Never read, print, move or commit the signing private key. It lives outside
  the repository; `catalog:sign` reads its path from `SECRETHUB_CATALOG_PRIVATE_KEY`
  and refuses any path inside the repo.
- Only the fields `id`, `label`, `capabilities`, `deprecated` (per model) and
  `defaultModel` (per provider) are allowed. Capabilities come from the fixed
  vocabulary: chat, code, reasoning, vision, image_gen, video_gen, tts, asr,
  embedding, rerank. Providers must already exist in `providers.json`; adding a
  provider needs an app release, not a catalog update.

## Workflow

1. **Collect.** For each provider in `providers.json` that has models, open its
   official model page or docs, and compare with
   `packages/provider-definitions/model-catalog.json`. Note new models, retired
   models, renamed ids, a new recommended default, and capability changes.
2. **Propose.** Reply with one table and wait for approval:

   | Provider | Added | Retired (deprecated) | Default model change | Capability change | Source |
   |---|---|---|---|---|---|

   Keep the list short; skip providers with no change and say so in one line.
3. **Edit after approval.** Update `model-catalog.json`:
   - A provider's `models` list replaces the old list entirely, so keep every
     model that should still be recommended.
   - Keep a retired model with `"deprecated": true` only when its capability
     tags are still useful for recognising it; otherwise drop it.
   - `defaultModel` must be a listed, non-deprecated model.
   - Increase `serial` (use `YYYYMMDDNN`, always larger than the current value)
     and set `publishedAt` to the current UTC time.
4. **Check.** Run `npm run catalog:check`. Fix every reported problem, then run
   `npm run catalog:test` if scripts or rules were touched.
5. **Sign (maintainer, on their machine).** Tell the maintainer to run
   `npm run catalog:sign` with `SECRETHUB_CATALOG_PRIVATE_KEY` pointing to their
   private key. The signed pair is written to `dist/catalog/`
   (`model-catalog.json` and `model-catalog.json.sig`). Do not run this step
   yourself and do not ask for the key.
6. **Upload (maintainer).** Upload both files to the hosting location behind
   `SECRETHUB_CATALOG_URL` (the `.sig` file sits next to the catalog with the
   same name plus `.sig`).
7. **Bake before the next app release.** Run `npm run catalog:bake`, review the
   `providers.json` and `catalog-serial.json` diff, and run the full test suite.
   This makes fresh installs start with the same data and raises the app's
   built-in anti-rollback serial.

## First-time setup (maintainer only)

Run `npm run catalog:keygen -- --private-out <path outside the repo>` once. It
writes the public key to `packages/provider-definitions/catalog-public-key.txt`
(commit it) and the private key to the given path (store it offline, for
example in a password manager; never commit it). After replacing the public
key, ship a new app version: only apps built with the new key accept catalogs
signed by it. If the private key is lost or leaked, repeat this step and
release a new version.
