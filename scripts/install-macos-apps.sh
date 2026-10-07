#!/bin/bash
# 校验并安装 5 个 ArtCraft 应用到 /Applications
# 用法: ./install-macos-apps.sh [repo ...]   (默认全部)
# 注意: macOS 自带 bash 3.2，不能用关联数组(declare -A)
set -uo pipefail
DIR="$HOME/artcraft-mobile/dmg"
APPS_DIR="/Applications"

if [ "$#" -eq 0 ]; then
  repos=(lightcraft photocraft filmcraft effectcraft printcraft)
else
  repos=("$@")
fi

cd "$DIR" || exit 1
for repo in "${repos[@]}"; do
  case "$repo" in
    lightcraft)  app=LightCraft;  ver=0.2.1 ;;
    photocraft)  app=PhotoCraft;  ver=0.3.0 ;;
    filmcraft)   app=FilmCraft;   ver=0.2.1 ;;
    effectcraft) app=EffectCraft; ver=0.4.0 ;;
    printcraft)  app=PrintCraft;  ver=0.2.1 ;;
    *) echo "未知仓库: $repo"; continue ;;
  esac
  dmg="$repo-$ver-macos-universal.dmg"
  echo "=== $app $ver ==="
  [ -s "$dmg" ] || { echo "  DMG 缺失: $dmg"; continue; }

  # 1) sha256 校验
  if [ -s "SHA256SUMS-$repo.txt" ]; then
    if grep "$dmg" "SHA256SUMS-$repo.txt" | shasum -a 256 -c -; then
      echo "  sha256 OK"
    else
      echo "  sha256 校验失败，跳过"; continue
    fi
  else
    echo "  无校验和文件，跳过校验"
  fi

  # 2) 挂载 + 拷贝
  hdiutil attach -nobrowse -quiet "$dmg" || { echo "  挂载失败"; continue; }
  vol="/Volumes/$app $ver"
  if [ -d "$vol/$app.app" ]; then
    rm -rf "$APPS_DIR/$app.app"
    cp -R "$vol/$app.app" "$APPS_DIR/" && echo "  已安装: $APPS_DIR/$app.app"
  else
    echo "  卷内未找到 $app.app；实际内容: $(ls "$vol" 2>/dev/null | head -3 | tr '\n' ' ')"
  fi
  hdiutil detach "$vol" -quiet 2>/dev/null

  # 3) 签名 / 公证校验
  if codesign --verify --deep --strict "$APPS_DIR/$app.app" 2>/dev/null; then echo "  codesign OK"; fi
  spctl -a -vvv --type exec "$APPS_DIR/$app.app" 2>&1 | head -3
  xcrun stapler validate "$dmg" 2>&1 | head -2
done
echo "=== 完成 ==="
