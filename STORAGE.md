# 磁盘分析与清理记录（2026-10-08）

## 一、本次已执行（可安全回收）

| 项目 | 释放 | 说明 |
|---|---|---|
| Homebrew 下载缓存 | **12.9 GB** | `brew cleanup --prune=all -s`，缓存目录 11 GB → 53 MB |
| Rust/Gradle 构建产物（debug 与 release 中间物） | **~9 GB** | `artcraft-mobile/{filmcraft,probe}/*/target`、debug `jniLibs`、debug APK、已安装完毕的 5 个 DMG |
| 应用缓存（pip、各类更新器、VSCode ShipIt） | ~1.5 GB | `~/Library/Caches/{pip,kimi-desktop-updater,@zcodedesktop-updater,@opencode-aidesktop-updater,com.microsoft.VSCodeInsiders.ShipIt}` |
| **合计** | **≈ 23 GB** | 磁盘剩余：11 GiB → 30 GiB |

## 二、磁盘现状（家目录 Top）

| 路径 | 占用 | 性质 |
|---|---|---|
| `~/Library` | 108 GB → 现约 86 GB | 见下 |
| `~/Downloads` | **78 GB** | 个人文件，未动 |
| `~/OrbStack` | 38 GB | 容器/虚拟机镜像，**不要动** |
| `~/Applications` | 23 GB | 应用本体 |
| `~/artcraft-mobile` | 2.6 GB（清理后） | 本次移植实验目录 |
| `~/wine-arm64-lab` / `~/PycharmProjects` / `~/armvenv313` / `~/toolchains` | 11 / 9.9 / 8.6 / 8.2 GB | 个人开发环境 |

`~/Library` 内：`Mobile Documents` 35 GB（iCloud 同步，动它会触发同步删除，**不要动**）、`Application Support` 35 GB、`Metadata` 6.7 GB（Spotlight 索引，可重建）、`Android` 10 GB、`Containers` 2.5 GB。

## 三、建议删除候选（**未执行，等你确认**）

| # | 路径 | 大小 | 风险 / 说明 |
|---|---|---|---|
| 1 | `~/Library/Android/sdk/ndk/26.1.10909125` | **3.0 GB** | 旧 NDK；本次工作用 r28.2（保留）。除非有别的项目钉住 r26，可删（随时能重装） |
| 2 | `~/Downloads/android-ndk-r26b-darwin.zip` | 939 MB | 与上面同源的安装包，已解压过，可删 |
| 3 | `~/Library/Application Support/JetBrains/*`（旧版本） | ~4 GB | `PyCharm2026.1` 1.1 GB + `WebStorm2026.1` 1.1 GB 等旧版本目录，IDE 会重建（仅当不再用旧版） |
| 4 | `~/Library/Metadata/CoreSpotlight` | 6.7 GB | Spotlight 索引；删后系统会重建（一段时间搜索变慢） |
| 5 | `~/Downloads` 里的大文件 | 78 GB | `assignment1-basics.zip` 11 GB、`proteinproject` 4.1 GB、`videoout` 4.0 GB、`photo2026.zip` 3.2 GB、`agentcomp` 2.0 GB… **都是你的文件，请自行确认** |
| 6 | `~/Library/Application Support/Steam` | 3.7 GB | 游戏数据 |
| 7 | `~/Library/Application Support/com.apple.container` + OrbStack | 0.9 + 38 GB | 容器镜像，删除会丢容器环境 |

## 四、保留（本次工作还要用）

- `~/Library/Android/sdk`（NDK r28.2、platform-tools、emulator、system-images）——继续做 Android 移植要用；
- `~/.cargo`（422 MB）、`~/.gradle`（615 MB）、`~/artcraft-mobile/tools/gradle-9.3.1`——构建链路；
- `~/artcraft-mobile/filmcraft`（clone + release 构建缓存）。

## 五、可复用的清理命令

```bash
brew cleanup --prune=all -s                        # Homebrew 缓存（本次释放 12.9 GB）
rm -rf ~/Library/Caches/pip                        # pip 缓存
cargo clean --manifest-path <项目>/Cargo.toml       # Rust 构建产物
rm -rf <gradle 项目>/app/build <项目>/target        # Gradle/Rust 中间物
sdkmanager --sdk_root=$HOME/Library/Android/sdk --uninstall "ndk;26.1.10909125"   # 旧 NDK（需确认）
```
