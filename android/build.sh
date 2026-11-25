#!/usr/bin/env bash
# Build the Ryoku Box Android APK with the Android SDK command-line tools.
set -euo pipefail

SDK="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-/home/gamanotsuffering/Android/Sdk}}"
BT="$SDK/build-tools/35.0.0"
AJAR="$SDK/platforms/android-35/android.jar"
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE"

[[ -f "$AJAR" && -x "$BT/aapt2" ]] || { echo "Android SDK 35 build tools not found at $SDK" >&2; exit 1; }

rm -rf build gen classes dex
mkdir -p build gen classes dex

echo "== baking auto-link config =="
TOKEN_FILE="${HOME}/.config/ryokubox/token"
TOKEN="$(tr -d '\r\n' < "$TOKEN_FILE" 2>/dev/null || true)"
LANIP="$(ip -4 -br addr show 2>/dev/null | awk '$3 !~ /^127\./ {sub(/\/.*/,"",$3); print $3; exit}')"
[ -n "$LANIP" ] || LANIP="127.0.0.1"
[ -n "$TOKEN" ] || { echo "No agent token at $TOKEN_FILE — start ryokubox once first." >&2; exit 1; }
mkdir -p gen/com/ryoku/ryokubox
cat > gen/com/ryoku/ryokubox/BuildConfig.java <<JAVA_EOF
package com.ryoku.ryokubox;
/** Generated on the PC at build time: auto link & pair credentials. */
final class BuildConfig {
    static final String TOKEN = "$TOKEN";
    static final String FALLBACK_HOST = "$LANIP:8875";
}
JAVA_EOF

echo "== compiling resources =="
"$BT/aapt2" compile --dir res -o build/res.zip

echo "== linking manifest =="
"$BT/aapt2" link -o build/base.apk -I "$AJAR" -R build/res.zip \
  --manifest AndroidManifest.xml --java gen \
  --min-sdk-version 24 --target-sdk-version 35

echo "== compiling java =="
javac -nowarn -source 8 -target 8 -encoding UTF-8 \
  -classpath "$AJAR" -d classes \
  $(find src gen -name '*.java' | sort)

echo "== dexing =="
"$BT/d8" --lib "$AJAR" --min-api 24 --output dex \
  $(find classes -name '*.class' | sort)

cp dex/classes.dex build/classes.dex
(cd build && jar uf base.apk classes.dex)

echo "== aligning and signing =="
"$BT/zipalign" -f 4 build/base.apk build/aligned.apk
if [[ ! -f debug.keystore ]]; then
  keytool -genkeypair -v -keystore debug.keystore -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=Ryoku Box Debug,O=Ryoku,C=US" >/dev/null 2>&1
fi
"$BT/apksigner" sign --ks debug.keystore --ks-pass pass:android \
  --key-pass pass:android --out RyokuBox.apk build/aligned.apk
"$BT/apksigner" verify RyokuBox.apk
ls -lh RyokuBox.apk
