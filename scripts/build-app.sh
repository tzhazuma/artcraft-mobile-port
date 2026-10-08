#!/bin/bash
# Build one ArtCraft Android port's Rust library. Usage: build-app.sh <app>
#   build-app.sh lightcraft   → apps/lightcraft-android/jniLibs/arm64-v8a/libmain.so
#
# Wraps the whole thing in the team build lock (macOS has no flock(1); the lock uses
# python3 fcntl). Exits with the cargo status, so never pipe this through `tail`.
set -uo pipefail

APP="${1:?usage: build-app.sh <app>   e.g. lightcraft}"
REPO="$HOME/artcraft-mobile"
CLONE="$REPO/$APP"
PKG="$APP-android"

[ -d "$CLONE" ] || { echo "FAIL: no clone at $CLONE"; exit 2; }

export PATH="$HOME/.cargo/bin:$HOME/Library/Android/sdk/platform-tools:$PATH"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/28.2.13676358"
export JAVA_HOME=/opt/homebrew/opt/openjdk@21

# Patch is the source of truth — always re-mirror before building.
rsync -a "$REPO/patches/$APP-android/" "$CLONE/apps/$PKG/" || exit 2

# Profile choice. `--release` is right when the app's release profile leaves `debug` unset
# (no debug_info → ~46 MB .so, like filmcraft). An app that sets `debug = "line-tables-only"`
# in release (lightcraft) must use its own `dist` profile, or the .so balloons to ~234 MB.
PROFILE_FLAG="--release"
RELEASE_BLOCK="$(awk '/^\[profile\.release\]/{f=1;next} f&&/^\[/{f=0} f' "$CLONE/Cargo.toml")"
if printf '%s' "$RELEASE_BLOCK" | grep -qE '^[[:space:]]*debug[[:space:]]*=' \
   && grep -q '^\[profile\.dist\]' "$CLONE/Cargo.toml"; then
  PROFILE_FLAG="--profile dist"
fi
echo "== profile: $PROFILE_FLAG"

LOG="/tmp/build-$APP.log"
echo "== building $PKG (log: $LOG)"

# Generate the real build script into a temp file with a QUOTED heredoc so that nothing inside
# is expanded here. Never pass a command string through `bash -c "..."` with embedded quotes:
# the outer expansion silently mangles PATH and then nothing (not even `cc`) resolves.
#
# `mktemp -t` picks a unique name in $TMPDIR; the explicit `$$` suffix removes the template race
# that two concurrent teammates hit (the second mktemp failed, INNER stayed empty, and the lock
# then ran an empty command — reported as a bogus "compile error"). `set -e` makes any failure
# here abort loudly instead of building nothing.
set -e
INNER="$(mktemp -t "artcraft-inner-$$")"
[ -n "$INNER" ] && [ -f "$INNER" ] || { echo "FAIL: could not create temp build script"; exit 2; }
cat > "$INNER" <<'INNER_EOF'
#!/bin/bash
set -uo pipefail
cd "__CLONE__"
export PATH="$HOME/.cargo/bin:$PATH"
export ANDROID_HOME="__ANDROID_HOME__"
export ANDROID_NDK_HOME="__NDK__"
export JAVA_HOME=/opt/homebrew/opt/openjdk@21
echo "[inner] PATH head: $(printf '%s' "$PATH" | cut -d: -f1-3)"
echo "[inner] cc: $(command -v cc || echo NOT_FOUND)"
exec cargo +stable ndk -t arm64-v8a -o "apps/__PKG__/jniLibs" build __PROFILE__ -p __PKG__
INNER_EOF
sed -i '' \
  -e "s|__CLONE__|$CLONE|g" \
  -e "s|__PKG__|$PKG|g" \
  -e "s|__PROFILE__|$PROFILE_FLAG|g" \
  -e "s|__ANDROID_HOME__|$ANDROID_HOME|g" \
  -e "s|__NDK__|$ANDROID_NDK_HOME|g" \
  "$INNER"
chmod +x "$INNER"

# The build itself may legitimately fail; `set -e` is only here to catch setup mistakes, so turn
# it off before invoking cargo or a compile error would abort before we can report it.
set +e
/tmp/artcraft-build-lock.sh "$INNER" > "$LOG" 2>&1
rc=$?
rm -f "$INNER"

echo "== cargo exit code: $rc"
if [ "$rc" -ne 0 ]; then
  echo "== errors =="
  grep -E '^error' -A6 "$LOG" | head -40
fi
echo "== artifact =="
SO="$CLONE/apps/$PKG/jniLibs/arm64-v8a/libmain.so"
if [ -f "$SO" ]; then ls -la "$SO"; else echo "(no libmain.so produced)"; fi
exit $rc
