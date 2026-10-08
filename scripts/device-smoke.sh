#!/bin/bash
# Real-device / emulator smoke test for an ArtCraft Android port.
# Usage: device-smoke.sh <app-id> <activity> <tag> <serial|auto> [shot-name]
#   e.g. device-smoke.sh ai.storyteller.lightcraft .MainActivity LightCraft auto lightcraft-real
#
# Installs the newest APK matching <tag>, launches it, greps logcat for the tag /
# panics, and pulls a screenshot into shots/. Exits non-zero when unhealthy.
set -uo pipefail

APP_ID="${1:?app id}"; ACTIVITY="${2:?activity}"; TAG="${3:?apk/log tag}"
SERIAL="${4:-auto}"; SHOT="${5:-${TAG}-real}"
# Optional 6th arg: exact APK path (bypasses tag matching — preferred right after a build).
APK_ARG="${6:-}"

export PATH="$HOME/Library/Android/sdk/platform-tools:$PATH"
ADB="${ANDROID_HOME:-$HOME/Library/Android/sdk}/platform-tools/adb"
REPO="$HOME/artcraft-mobile"

if [ "$SERIAL" = "auto" ]; then
  SERIAL="$("$ADB" devices | awk '/\tdevice$/{print $1}' | grep -v '^emulator-' | head -1)"
  [ -z "$SERIAL" ] && SERIAL="$("$ADB" devices | awk '/\tdevice$/{print $1}' | head -1)"
fi
[ -z "$SERIAL" ] && { echo "FAIL: no online device"; "$ADB" devices -l; exit 2; }
echo "== device: $SERIAL"

if [ -n "$APK_ARG" ]; then APK="$APK_ARG"; else
  APK="$REPO/dist/$(basename "$(ls -t "$REPO"/dist/*"$TAG"*.apk 2>/dev/null | head -1)" 2>/dev/null)"
  [ ! -f "$APK" ] && APK="$(ls -t "$REPO"/*/apps/*-android/android/app/build/outputs/apk/release/*"$TAG"*.apk 2>/dev/null | head -1)"
  [ ! -f "$APK" ] && APK="$(ls -t "$REPO"/*/apps/*-android/android/app/build/outputs/apk/release/*.apk 2>/dev/null | head -1)"
fi
[ ! -f "$APK" ] && { echo "FAIL: no APK found matching '$TAG'"; exit 2; }
echo "== apk: $APK ($(stat -f%z "$APK") bytes)"

echo "== install"; "$ADB" -s "$SERIAL" install -r "$APK" || { echo "FAIL: install"; exit 3; }

echo "== launch"; "$ADB" -s "$SERIAL" logcat -c
"$ADB" -s "$SERIAL" shell am start -n "$APP_ID/$ACTIVITY" || { echo "FAIL: am start"; exit 4; }
sleep 12

LOG="$("$ADB" -s "$SERIAL" logcat -d | grep -Ei "$TAG|panick|AndroidRuntime|FATAL|MediaCodec" | tail -40)"
echo "== logcat"; echo "${LOG:-<no matching lines>}"

echo "== verdict"
if echo "$LOG" | grep -Eqi "FATAL|AndroidRuntime|panick"; then echo "FAIL: crash detected"; RC=1
elif [ -z "$LOG" ]; then echo "WARN: no matching log lines"; RC=1
else echo "PASS: launched, no crash"; RC=0; fi

mkdir -p "$REPO/shots"
"$ADB" -s "$SERIAL" exec-out screencap -p > "$REPO/shots/${SHOT}.png" 2>/dev/null
if [ -s "$REPO/shots/${SHOT}.png" ]; then
  echo "== shot: shots/${SHOT}.png ($(stat -f%z "$REPO/shots/${SHOT}.png") bytes)"
else echo "WARN: screenshot empty"; RC=1; fi
exit $RC
