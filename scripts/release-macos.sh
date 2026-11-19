#!/usr/bin/env bash
# Build, sign and publish a SecretHub macOS release (.dmg for first installs, .app.tar.gz + latest-macos.json
# for the in-app updater). macOS counterpart of scripts/release-windows.ps1.
#
#   1. reads the version from apps/desktop/src-tauri/tauri.conf.json
#   2. npm --prefix apps/mcp run build:sea         (standalone MCP sidecar for this Mac's architecture)
#   3. npm --prefix apps/desktop run tauri build   (Developer ID signing + notarization of the .app,
#                                                   updater artifacts signed with the updater private key)
#   4. signs, notarizes and staples the .dmg
#   5. writes latest-macos.json                    (the endpoint set in tauri.macos.conf.json)
#   6. uploads with scp, manifest last             (skipped with --dry-run: commands are only printed)
#
# The updater private key never lives in the repo: it is the same file the Windows releases use, copied to
# ~/.tauri/secrethub-updater.key (or pass --key PATH). Mac and Windows have separate manifests, so either
# platform can ship a version without the other.
#
# Apple signing: the "Developer ID Application" certificate in the login keychain is found automatically
# (or set APPLE_SIGNING_IDENTITY). Notarization uses an App Store Connect API key; its settings live outside
# the repo in ~/.config/secrethub-release/macos.env (override with SECRETHUB_MACOS_ENV):
#   APPLE_API_ISSUER=<Issuer ID>  APPLE_API_KEY=<Key ID>  APPLE_API_KEY_PATH=<path to AuthKey_<Key ID>.p8>
# --adhoc builds without them (ad-hoc signature, not notarized: Gatekeeper blocks it on other Macs).
#
#   bash scripts/release-macos.sh --notes "..." --dry-run
set -euo pipefail

SSH_HOST=tx-bt
REMOTE_DIR=/www/wwwroot/updates.9aizhuan.com/secrethub
BASE_URL=https://updates.9aizhuan.com/secrethub
KEY_PATH="$HOME/.tauri/secrethub-updater.key"
NOTES=""
DRY_RUN=0
SKIP_BUILD=0
ADHOC=0
while [ $# -gt 0 ]; do
  case "$1" in
    --notes) NOTES="$2"; shift 2 ;;
    --key) KEY_PATH="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --adhoc) ADHOC=1; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

REPO="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(node -p "require('$REPO/apps/desktop/src-tauri/tauri.conf.json').version")"
case "$(uname -m)" in
  arm64) ARCH=aarch64 ;;
  x86_64) ARCH=x86_64 ;;
  *) echo "unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac
[ -f "$KEY_PATH" ] || { echo "updater private key not found: $KEY_PATH" >&2; exit 1; }

ENV_FILE="${SECRETHUB_MACOS_ENV:-$HOME/.config/secrethub-release/macos.env}"
if [ "$ADHOC" = 0 ]; then
  # shellcheck disable=SC1090
  [ -f "$ENV_FILE" ] && { set -a; . "$ENV_FILE"; set +a; }
  if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
    APPLE_SIGNING_IDENTITY="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p' | head -n 1)"
  fi
  [ -n "$APPLE_SIGNING_IDENTITY" ] || { echo "no 'Developer ID Application' certificate in the keychain (use --adhoc for an unsigned test build)" >&2; exit 1; }
  for var in APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH; do
    [ -n "${!var:-}" ] || { echo "$var is not set (expected in $ENV_FILE)" >&2; exit 1; }
  done
  [ -f "$APPLE_API_KEY_PATH" ] || { echo "App Store Connect key not found: $APPLE_API_KEY_PATH" >&2; exit 1; }
  # tauri reads these: the identity overrides signingIdentity "-" from tauri.macos.conf.json.
  export APPLE_SIGNING_IDENTITY APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH
  echo "signing as: $APPLE_SIGNING_IDENTITY (notarized)"
else
  unset APPLE_SIGNING_IDENTITY APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH
  echo "signing: ad-hoc, NOT notarized"
fi
echo "SecretHub $VERSION darwin-$ARCH (dry-run=$DRY_RUN)"

if [ "$SKIP_BUILD" = 0 ]; then
  echo "==> build MCP sidecar"
  npm --prefix "$REPO/apps/mcp" run build:sea
  echo "==> tauri build"
  # The key has no password; the CLI still needs the (empty) password variable or it prompts.
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY_PATH")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    npm --prefix "$REPO/apps/desktop" run tauri build
fi

BUNDLE="$REPO/target/release/bundle"
APP_TGZ="$BUNDLE/macos/SecretHub.app.tar.gz"
DMG="$BUNDLE/dmg/SecretHub_${VERSION}_${ARCH}.dmg"
for file in "$APP_TGZ" "$APP_TGZ.sig" "$DMG"; do
  [ -f "$file" ] || { echo "missing build output: $file" >&2; exit 1; }
done
if [ "$ADHOC" = 0 ] && [ "$SKIP_BUILD" = 0 ]; then
  # tauri notarizes and staples the .app; the dmg around it gets the same treatment.
  echo "==> sign + notarize dmg"
  codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$DMG"
  xcrun notarytool submit "$DMG" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" --wait
  xcrun stapler staple "$DMG"
fi
if [ "$ADHOC" = 0 ]; then
  echo "==> Gatekeeper check"
  spctl --assess --type execute -vv "$BUNDLE/macos/SecretHub.app"
  spctl --assess --type open --context context:primary-signature -vv "$DMG"
fi
UPDATE_NAME="SecretHub_${VERSION}_${ARCH}.app.tar.gz"
DMG_NAME="$(basename "$DMG")"

MANIFEST="$BUNDLE/latest-macos.json"
VERSION="$VERSION" NOTES="${NOTES:-SecretHub $VERSION}" SIG="$(cat "$APP_TGZ.sig")" \
URL="$BASE_URL/$UPDATE_NAME" PLATFORM="darwin-$ARCH" node -e '
const { VERSION, NOTES, SIG, URL, PLATFORM } = process.env;
const manifest = { version: VERSION, notes: NOTES, pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
  platforms: { [PLATFORM]: { signature: SIG.trim(), url: URL } } };
require("fs").writeFileSync(process.argv[1], JSON.stringify(manifest, null, 2) + "\n");
' "$MANIFEST"
echo "wrote $MANIFEST"

run() {
  if [ "$DRY_RUN" = 1 ]; then echo "[dry-run] $*"; else echo "==> $*"; "$@"; fi
}
# Update payload and dmg first, manifest last, so clients never see a manifest pointing at a missing file.
run scp "$APP_TGZ" "$SSH_HOST:$REMOTE_DIR/$UPDATE_NAME"
run scp "$DMG" "$SSH_HOST:$REMOTE_DIR/$DMG_NAME"
run scp "$MANIFEST" "$SSH_HOST:$REMOTE_DIR/latest-macos.json"
# Stable download link for websites and docs: always points at the newest dmg.
run ssh "$SSH_HOST" "ln -sfn '$DMG_NAME' '$REMOTE_DIR/SecretHub-latest-$ARCH.dmg'"
echo "download: $BASE_URL/SecretHub-latest-$ARCH.dmg"
[ "$DRY_RUN" = 1 ] && echo "done (dry run, nothing uploaded)" || echo done
