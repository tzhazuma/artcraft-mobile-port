#!/bin/bash
# 下载 5 个 ArtCraft 应用的 macOS universal DMG 与校验和文件
set -uo pipefail
DIR="$HOME/artcraft-mobile/dmg"
mkdir -p "$DIR"; cd "$DIR" || exit 1

apps=(
  "lightcraft v0.2.1 0.2.1"
  "photocraft v0.3.0 0.3.0"
  "filmcraft v0.2.1 0.2.1"
  "effectcraft v0.4.0 0.4.0"
  "printcraft v0.2.1 0.2.1"
)

for entry in "${apps[@]}"; do
  read -r repo tag ver <<<"$entry"
  f="$repo-$ver-macos-universal.dmg"
  url="https://github.com/storytold/$repo/releases/download/$tag/$f"
  sums="SHA256SUMS-$repo.txt"
  echo "=== $repo $ver ==="
  # 校验和文件（小，先拿）
  [ -s "$sums" ] || curl -fsSL --connect-timeout 10 --max-time 120 -o "$sums" \
    "https://gh-proxy.com/https://github.com/storytold/$repo/releases/download/$tag/SHA256SUMS.txt" \
    && echo "  sums ok" || echo "  sums FAILED"
  # DMG：直连 -> gh-proxy -> ghproxy.net 依次尝试，支持续传
  for src in "direct" "gh-proxy" "ghproxy.net"; do
    case "$src" in
      direct)       u="$url" ;;
      gh-proxy)     u="https://gh-proxy.com/$url" ;;
      ghproxy.net)  u="https://ghproxy.net/$url" ;;
    esac
    if [ -f "$f" ] && [ "$(stat -f%z "$f" 2>/dev/null || echo 0)" -gt 30000000 ]; then
      echo "  [$src] already complete: $(stat -f%z "$f") bytes"; break
    fi
    echo "  [$src] downloading ..."
    if curl -fL --retry 2 --retry-delay 3 --connect-timeout 10 --max-time 1800 -C - -o "$f" "$u"; then
      echo "  [$src] OK: $(stat -f%z "$f") bytes"; break
    else
      echo "  [$src] FAILED"
    fi
  done
done
echo "=== download phase done ==="
ls -l "$DIR"
