#!/usr/bin/env bash
# M5 (#23): builds the Android app from the same code base as the desktop app.
#
#   apps/desktop/android.sh OUT.apk
#
# Needs the Android SDK and NDK (ANDROID_HOME, NDK_HOME or ANDROID_NDK_LATEST_HOME as on GitHub
# runners), Java 17, Node, trunk and the Rust target aarch64-linux-android. The Gradle project in
# gen/android is generated, not committed (like the rest of gen/). The APK is built for arm64
# (nearly every phone since 2017) and signed with the keystore in ANDROID_KEYSTORE (a file),
# ANDROID_KEYSTORE_PASSWORD and ANDROID_KEY_ALIAS.
set -euo pipefail

out="$(realpath -m "${1:?Aufruf: android.sh OUT.apk}")"
: "${ANDROID_HOME:?ANDROID_HOME fehlt}"
: "${ANDROID_KEYSTORE:?ANDROID_KEYSTORE fehlt (Pfad zum Keystore)}"
: "${ANDROID_KEYSTORE_PASSWORD:?ANDROID_KEYSTORE_PASSWORD fehlt}"
: "${ANDROID_KEY_ALIAS:?ANDROID_KEY_ALIAS fehlt}"
export NDK_HOME="${NDK_HOME:-${ANDROID_NDK_LATEST_HOME:?NDK_HOME fehlt}}"

cd "$(dirname "$0")"
tauri() { npx --yes @tauri-apps/cli@2 "$@"; }

if [ ! -d gen/android ]; then
  tauri android init --ci
fi
# The app's own icons instead of the template's.
cp -r icons/android/. gen/android/app/src/main/res/

tauri android build --ci --apk --target aarch64

unsigned="$(find gen/android/app/build/outputs/apk -name '*-release-unsigned.apk' | head -n 1)"
[ -n "$unsigned" ] || { echo "kein APK gebaut" >&2; exit 1; }
tools="$(ls -d "$ANDROID_HOME"/build-tools/* | sort -V | tail -n 1)"
aligned="$(mktemp --suffix=.apk)"
"$tools/zipalign" -f -p 4 "$unsigned" "$aligned"
"$tools/apksigner" sign \
  --ks "$ANDROID_KEYSTORE" --ks-pass env:ANDROID_KEYSTORE_PASSWORD \
  --ks-key-alias "$ANDROID_KEY_ALIAS" --out "$out" "$aligned"
"$tools/apksigner" verify --print-certs "$out" | head -n 3
rm -f "$aligned"
echo "APK: $out"
