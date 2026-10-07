#!/bin/bash
# 校验并安装 5 个 ArtCraft 应用到 /Applications
# 用法: ./install-macos-apps.sh [repo ...]   (默认全部)
set -uo pipefail
DIR="$HOME/artcraft-mobile/dmg"
APPS_DIR="/Applications"

declare -A APPNAME=( [lightcraft]=LightCraft [photocraft]=PhotoCraft [filmcraft]=FilmCraft [effectcraft]=EffectCraft [printcraft]=PrintCraft )
declare -A VERS=( [lightcraft]=0.2.1 [photocraft]=0.3.0 [filmcraft]=0.2.1 [effectcraft]=0.4.0 [printcraft]=0.2.1 )

repos=("$@"); [ ${#repos[@]} -eq 0 ] && repos=(lightcraft photocraft filmcraft effectcraft printcraft)

cd "$DIR" || exit 1
for repo in "${repos[@]}"; do
  app="${APPNAME[$repo]}"; ver="${VERS[$repo]}"
  dmg="$repo-$ver-macos-universal.dmg"
  echo "=== $app $ver ==="
  [ -s "$dmg" ] || { echo "  DMG 缺失: $dmg"; continue; }
  # 1) 校验 sha256
  if [ -s "SHA256SUMS-$repo.txt" ]; then
    if grep "$dmg" "SHA256SUMS-$repo.txt" | shasum -a 256 -c -; then echo "  sha256 OK"; else echo "  sha256 校验失败，跳过"; continue; fi
  else
    echo "  无校验和文件，跳过校验"
  fi
  # 2) 挂载 + 拷贝
  hdiutil attach -nobrowse -quiet "$dmg" || { echo "  挂载失败"; continue; }
  vol="/Volumes/$app $ver"
  if [ -d "$vol/$app.app" ]; then
    rm -rf "$APPS_DIR/$app.app"
    cp -R "$vol/$app.app" "$APPS_DIR/" && echo "  已安装到 $APPS_DIR/$app.app"
  else
    echo "  卷内未找到 $app.app（实际内容：$(ls "$vol" 2>/dev/null | head -3 | tr '\n' ' ')）"
  fi
  hdiutil detach "$vol" -quiet 2>/dev/null
  # 3) 签名/公证校验
  codesign --verify --deep --strict "$APPS_DIR/$app.app" 2>&1 | head -2 && echo "  codesign OK"
  spctl -a -vvv --type exec "$APPS_DIR/$app.app" 2>&1 | head -3
  xcrun stapler validate "$dmg" 2>&1 | head -3
done
echo "=== 完成 ==="
