#!/usr/bin/env bash
# Install the exact Android build tools this project needs.
# Run this on the PC before ./android/build.sh. Nothing here is committed as
# binaries: the Android SDK is ~280 MB and is not redistributable in a repo.
set -euo pipefail

SDK="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Android/Sdk}}"
CMDLINE_ZIP="commandlinetools-linux-11076708_latest.zip"
CMDLINE_URL="https://dl.google.com/android/repository/$CMDLINE_ZIP"
TMP="$(mktemp -d)"

echo "SDK path: $SDK"
mkdir -p "$SDK/cmdline-tools"

if [[ ! -x "$SDK/cmdline-tools/latest/bin/sdkmanager" ]]; then
  echo "== downloading Android command-line tools =="
  curl -fsSL "$CMDLINE_URL" -o "$TMP/cmdline.zip"
  (cd "$TMP" && unzip -q cmdline.zip)
  rm -rf "$SDK/cmdline-tools/latest"
  mv "$TMP/cmdline-tools" "$SDK/cmdline-tools/latest"
fi

SDKMANAGER="$SDK/cmdline-tools/latest/bin/sdkmanager"
chmod +x "$SDKMANAGER"

echo "== accepting licences =="
yes | "$SDKMANAGER" --licenses >/dev/null 2>&1 || true

echo "== installing platform 35 + build-tools 35.0.0 =="
"$SDKMANAGER" "platforms;android-35" "build-tools;35.0.0"

echo "== installed =="
"$SDKMANAGER" --list_installed 2>/dev/null | grep -E 'platforms;android-35|build-tools;35.0.0' || true
echo
echo "Done. Now run:"
echo "  export ANDROID_SDK_ROOT=$SDK"
echo "  cd $(cd "$(dirname "$0")/.." && pwd) && ./android/build.sh"
