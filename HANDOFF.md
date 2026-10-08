# HANDOFF — ArtCraft 移动端移植（FilmCraft → Android 试点）

本文档让一个**对本会话毫无记忆**的 agent 接手本仓库：现有什么、什么已验证、如何构建/安装、坏在哪、以及如何把同样的方法用到其他 ArtCraft 应用与平台。先通读一遍，再按 §8 的索引深入。

上游 = `github.com/storytold/*`（ArtCraft /「Crafting Apps」）：用纯 Rust 洁净室重写 Adobe 全家桶（FilmCraft = Premiere、LightCraft = Lightroom、PhotoCraft = Photoshop、EffectCraft = After Effects、PrintCraft = Acrobat），桌面端为 egui/eframe 0.36 + wgpu 30 + winit 0.30，媒体栈全纯 Rust（任何地方都不用 ffmpeg），架构基于命令/控制通道。上游**完全没有移动端代码**；本仓库就是移植。

## 1. 仓库结构

移动端移植的公开交付仓库（remote：`github.com/tzhazuma/artcraft-mobile-port`）。试点：**FilmCraft → Android**；其他应用尚无移动端（见 §6/§7）；其他平台未动。

| 路径 | 内容 | 入库？ |
|---|---|---|
| `README.md` | 仓库入口：macOS 安装 + 摘要 | 是 |
| `PORTING.md` | 移植总方案：三条路线 × 四个平台、各应用缝合点表、风险 | 是 |
| `EXPERIMENT.md` | FilmCraft→Android 实验逐阶段证据（stage 0–13，含 logcat 摘录） | 是 |
| `STORAGE.md` | 本机磁盘清理记录（哪些构建产物可删） | 是 |
| `patches/filmcraft-android/` | **交付物 1**：Android 入口 crate（`src/lib.rs`、`saf.rs`、`selftest.rs`）+ Gradle 工程 + 测试素材 | 是 |
| `patches/platform-mediacodec/` | **交付物 2**：上游 `crates/platform` 的 MediaCodec 后端（`src/mediacodec.rs`、`src/lib.rs`、`Cargo.toml`） | 是 |
| `shots/` | 逐阶段截图证据（`stage15-export-*.png`、`stage14-palette.png`、`stage13-palette.png`、`stage12-real-*.png` 等） | 是 |
| `scripts/` | `download-dmgs.sh`（5 个 macOS DMG + sha256）、`install-macos-apps.sh`（校验→挂载→装到 /Applications） | 是 |
| `dmg/` | `SHA256SUMS-{filmcraft,lightcraft,photocraft,effectcraft,printcraft}.txt`（DMG 安装后已删） | 是 |
| `filmcraft/` | **gitignore** 的上游 FilmCraft 克隆——真正被构建的目录树 | 否 |
| `probe/egui-android-probe/` | 最小 eframe+wgpu Android 宿主（stage 3）；Gradle 骨架的来源 | 是 |
| `filmcraft-web/`、`android-activity/`、`tools/` | stage-0 的 wasm 产物、`rust-mobile/android-activity` 参考克隆、手动下载的 Gradle 9.3.1 | 否 |
| `assets/saf-test.mp4` | SAF 导入验证用的合成测试片（36 KB） | 是 |
| `dist/` | 发布 APK 的存放处——在本交接之后创建；当前构建好的 APK 在 `filmcraft/apps/filmcraft-android/android/app/build/outputs/apk/release/app-release.apk` | 否 |

**记录的载体是 `patches/` + 文档 + `shots/`。** `filmcraft/` 是可丢弃的工作克隆：改 `patches/`，构建前再镜像进克隆（§4）。永远不要提交 `filmcraft/`（`.gitignore` 同时排除 `tools/`、`dmg/`、`*.so`、`**/jniLibs/`、`**/build/`）。

交接时状态：git HEAD 为 `d0a8e65`（"Real-device import verified end to end…"）。工作区另有**未提交但已验证**的改动：`patches/filmcraft-android/src/` 的 `lib.rs`/`saf.rs`/`selftest.rs`（长按命令面板；SAF 保存回传；10-bit 自检；导出监视重绘）与 `patches/platform-mediacodec/src/mediacodec.rs`（10-bit/P010 解码），以及新文件 `patches/filmcraft-android/assets/test-10bit.hevc` 和 stage13–stage15 的新截图。这些改动**会在本会话的最终提交中入库，之后以 `git log` 为准**（本段仅作历史交代）。`patches/` 与 `filmcraft/` 克隆**当前完全同步**（`diff -rq` 核对）。另：该 crate 只从 `src/` 构建——crate 根目录下曾经的 `lib.rs`/`selftest.rs` 过期副本已删除，根目录不再有这两个文件。

上游克隆固定在 commit `8fcad73`（2026-10-07，版本 0.2.1）。若克隆丢失：`git clone https://github.com/storytold/filmcraft ~/artcraft-mobile/filmcraft`。网络注意：本机 `raw.githubusercontent.com` 被重置，下载走 `gh-proxy.com` / `ghproxy.net`，git 走 HTTPS/SSH-443。

### 五个应用的上游固定 commit（2026-10-08 记录）

全部 `git clone --depth 1 https://github.com/storytold/<app>`，均被 `.gitignore` 排除：

| 应用 | 上游仓库 | 固定 commit | MSRV | 内部 crate 名 |
|---|---|---|---|---|
| FilmCraft | `storytold/filmcraft` | `8fcad73` | 1.95 | `filmcraft` |
| LightCraft | `storytold/lightcraft` | `629e393` | 1.90 | `lightcraft` |
| PhotoCraft | `storytold/photocraft` | `5896f0b` | 1.95 | `photocraft` |
| EffectCraft | `storytold/effectcraft` | `72a2d47` | 1.95 | `effectcraft` |
| PrintCraft | `storytold/printcraft` | `1e54e70` | 1.90 | **`pdfcraft`**（仓库名与 crate 名不一致！） |

五个克隆结构一致：`crates/*` + `apps/*` + `xtask`，均带 `AGENTS.md`。

### 团队并行移植：两个必须知道的设施

**`BUILD-SERIALIZATION.md` + `/tmp/artcraft-build-lock.sh`** —— 磁盘只有 ~14 GB 余量时四个并行构建必然爆盘（每个应用 `target` ~2 GB + Gradle ~0.5 GB）。开发可并行，**构建必须串行**，用该脚本包住每条 `cargo ndk` / `gradle assembleRelease`。

> **陷阱（已踩）**：**macOS 没有 `flock(1)`**。该脚本第一版用 `flock 200` 且没写 `set -e`，于是打印 "ACQUIRED" 却根本没上锁——串行化静默失效。现版本改用 `python3 -c 'fcntl.flock(...)'` + `os.execvp` 让锁跨越 exec 存活，并已用「第二个获取者阻塞全程」验证互斥。若构建无输出地卡住，先查 `pgrep -fl artcraft-build-lock`——锁可能被挂死的构建占着。

**`scripts/device-smoke.sh`** —— 安装 + 启动 + 过滤 logcat + 截图一条龙，已在真机 vivo PA2573 上用 FilmCraft APK 端到端验证（`Displayed ai.storyteller.filmcraft/.MainActivity for user 0: +145ms`，截图 136 KB）：
```bash
scripts/device-smoke.sh <app-id> .MainActivity <Tag> auto <shot-name> [exact-apk-path]
```

### 磁盘清理记录（本次会话回收 ~7 GB，14 GB → 21 GB）

| 删除项 | 大小 | 理由 |
|---|---|---|
| `~/Library/Android/sdk/ndk/26.1.10909125` | 3.0 GB | 旧 NDK，本工作全部用 r28.2（STORAGE.md 早已批准） |
| `~/.npm/_cacache`、`~/Library/Caches/Homebrew/*` | 1.2 GB | 纯下载缓存，可再生 |
| `filmcraft/target` | 2.0 GB | 试点已完成，APK 已在 `dist/` |
| `filmcraft/apps/filmcraft-android/android/app/build` | 0.5 GB | 同上 |
| `probe/egui-android-probe/target`、`tools/gradle-9.3.1-bin.zip` | 0.2 GB | 均可再生 |

**不要删**：`dist/`、`shots/`、`tools/gradle-9.3.1/`（wrapper 下不动，必须手工 Gradle）、`~/.gradle/caches`（代理环境重取困难）、NDK 28.2、`~/.android/avd/`。


## 2. 已验证状态

### 2.1 `patches/filmcraft-android/` —— FilmCraft 已在 Android 上运行

- **GameActivity 上的 eframe/winit 外壳**（不是 NativeActivity）：`apps/filmcraft-android`，`[lib] name = "main"` / `cdylib`，`android.app.lib_name = "main"`，应用 id **`ai.storyteller.filmcraft`**，Activity `.MainActivity`，minSdk 31 / target 35 / Java 17 / AGP 9.1.0 / Gradle 9.3.1，`androidx.games:games-activity:4.4.0`。复用**未经修改**的上游 `filmcraft_engine::Session` + `filmcraft_ui_egui::FilmcraftApp`。
- **TouchShell**（`src/lib.rs`）：转发 `logic`/`ui`；第一帧之后（即 FilmCraft 自己的主题装完之后）设置 `interact_size = 40 pt`（约 105 px）、更大的 `button_padding`/`item_spacing`、`slider_width ≥ 160`、手指停住才显示 tooltip。故意**不做缩放**（`TOUCH_SCALE = 1.0`）：1080×2400 @420 dpi 手机上逻辑视口只有 411×914 pt，而桌面布局按 ≥900×560 pt 设计；放大反而更糟，解法是重排布局而非缩放（EXPERIMENT.md §5.5）。
- **CJK 字体回退**（`install_system_font`）：读 `/system/fonts/NotoSansCJK-Regular.ttc`（依次 NotoSerifCJK、DroidSansFallback），追加到现有字体族——必须在主题字体生效之后（`ctx.fonts(|f| f.definitions().clone())` 读-改-写），否则 egui 因 `FontFamily::Name("semibold")` 无绑定而 panic。
- **长按命令面板**：在屏幕顶部 8 % 长按 500 ms 打开全宽 File 命令面板（open/import/save/save-as/demo/close），经应用控制通道发 `engine.execute`。已验证：`shots/stage13-palette.png`。
- **手机工作区**经应用自己的 API 安装——`FilmcraftApp::set_workspaces(WorkspacePrefs { saved: [SavedWorkspace { name: "Phone", layout }], current: "Phone" })`，布局 `DockNode::Split { vertical, Ratio(0.42), Program / Tabs[Timeline, Project, Tools] }`；未改任何上游面板代码。已验证：`shots/stage12-real-project.png`（可见 PHONE 工作区按钮）。
- **SAF 导入走应用自己的菜单**（`src/saf.rs` + Java `SafBridge`/`MainActivity`）：`HostHooks.pick_files / pick_open_project / pick_open_file` 异步启动选择器并在当帧返回"没有文件"；Java 把所选文档复制进 `<filesDir>/import/` 并写 `<filesDir>/import-manifest.txt`；后台线程轮询 `safIsDone()` 后发送 `engine.execute {"command":"file.import","params":{"paths":[…]}}`——就是桌面 File ▸ Import 的同一条命令。真机（vivo）端到端验证：选择器 → Project 面板出现 `1_saf-test.mp4`（0:02，真实缩略图）。
- **SAF 保存/导出**（`pick_save` / `pick_save_as`）：hook 返回应用外部 `exports/` 目录里的临时路径，监视线程弹 `ACTION_CREATE_DOCUMENT`，等文件写完（大小稳定 ≥1.5 s）后拷贝到用户选定的 URI；用户取消则文件留在 exports 目录，不丢。已验证：文件落在用户选择的位置。`pick_folder` 仍返回应用自己的 exports 目录（SAF 目录树未接——见 §6）。
- **启动自检（三片素材）**：`src/selftest.rs` 内嵌 H.264、HEVC、HEVC Main10 三段小片（`test.h264` / `test.hevc` / `test-10bit.hevc`），启动时经引擎自己的工厂路径解码；Main10 还会与纯 Rust 软解逐像素比对（检验 P010 的 16-bit 输出转换，而非只数帧）。真机三段均 30/30 帧。
- **导出验证**（真机）：命令面板以显式参数发 `file.exportMedia`（前 2 s、H.264、`wait:false`），连续三次运行产物逐字节相同；产物经 ffprobe 核对通过。细节与 logcat 见 §3。
- **导出监视重绘修复**：空闲编辑器不会自己重绘，监视线程的 `jobs.list` 轮询就得不到应答；现在发出 `file.exportMedia` 后立即重绘、监视循环每轮也重绘（`src/lib.rs`），导出进度与完成帧可见。
- 体积：release `.so` **45–47 MB**（debug 855 MB，绝不发布 debug），APK **约 42–44 MB**（本次最终 44,535,267 B，sha256 `dc70a7472a6e104b91455407bcba351dfeb64d7c4f264d8727755e3937b504cb`），debug 签名所以 `adb install -r` 通吃。16 KB 页对齐 ✓（`zipalign -c -P 16`、`.so` 的 LOAD 段 0x4000）。

### 2.2 `patches/platform-mediacodec/` —— 硬解走引擎自己的路径

- `crates/platform::register()` 在 Android 上返回 `Available("MediaCodec")`，注册与 macOS `videotoolbox_factory` 完全同形的 `mediacodec_factory`；工厂用上游 `HybridDecoder` 包装，中途失败自动切回纯 Rust 解码器并走现成的 `note_hw_fallback()` 计数。
- 解码器支持 **H.264 与 HEVC，8/10-bit、4:2:0、渐进**；avcC → Annex-B；`csd-0` = SPS（H.264）或 VPS+SPS+PPS（HEVC）；输出按 `color-format`/`stride`/`slice-height`/`crop` 读取：8-bit 的 NV12 去交织成与软解一致的 `PixelData::Yuv8` 平面；EOS 用带 flag 的空输入缓冲排空。
- **10-bit 输出**：Main 10 的缓冲是 P010 → `PixelData::Yuv16 { bits: 10 }`（与 macOS `videotoolbox` 后端同一约定）。**4:2:2 已考察**：手机解码器不提供 4:2:2 profile/color-format——`/tmp/dumpsys-phone.txt` 里 `c2.mtk.hevc.decoder` 只宣告 4:2:0 profile + `YUVP010`，所以 4:2:2 保持软解。
- 拒绝策略（返回 `Err` → 软解回退 + `note_hw_declined()`）：非 H.264/HEVC、>10 bit、luma/chroma 位深不一致、非 4:2:0、隔行、缺参数集，以及设备上只有软件解码器（`c2.android.*` / `*.sw.*`）。
- 唯一的 `unsafe` 是 `unsafe impl Send for SendCodec`（ADR-0001 风格，带 `// SAFETY:`）；其余全部用安全的 `ndk` crate MediaCodec 绑定。
- **实测解码**：模拟器 `c2.goldfish.h264.decoder`；真机 vivo PA2573 / Android 16 → **`c2.mtk.avc.decoder`（H.264）与 `c2.mtk.hevc.decoder`（HEVC），各 30/30 帧，计数 `帧 30 会话 1 拒绝 0 回退 0`**。启动自检走引擎自己的 `filmcraft_codecs::make_video_decoder()` 路径，因此同时证明工厂注册、HybridDecoder 包装与 hw 计数都在工作：

  ```
  filmcraft-android: android_main entered
  filmcraft-android: hardware decoding: Available("MediaCodec")
  filmcraft_platform::mediacodec: MediaCodec: decoding 320x240 H.264 with c2.mtk.avc.decoder
  filmcraft-android: 自检: 解码器 MediaCodec H.264 | 样本 30 个 → 解出 30 帧（319x239） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0 ／ HEVC …
  ```

- **明确一句**：FilmCraft 的导出编码器是应用自带的纯 Rust 编码器；`crates/platform` 里**没有编码器**，所以 MediaCodec 硬件编码没有接——在写出编码后端之前，MTK 硬编不会被使用。

### 2.3 环境已验证

- 五个 macOS 应用均可安装（`scripts/install-macos-apps.sh` → `/Applications/{FilmCraft,LightCraft,PhotoCraft,EffectCraft,PrintCraft}.app`）。
- 工具链：rustup stable 1.99.0 + cargo-ndk 4.1.2、NDK `28.2.13676358`、手动 Gradle 9.3.1、JDK 21（Homebrew）、AGP 9.1.0、AVD `Medium_Phone_API_36.0`（arm64，API 36，Play 镜像）。

## 3. 10-bit 解码与导出验证（✅ 已验证）

两项均已在本会话内完成；详细证据（logcat 摘录）见 `EXPERIMENT.md` §5.10。

- **10-bit HEVC（Main 10）**：真机（15:38:34）`HEVC 10-bit 自检: 解码器 MediaCodec HEVC | 样本 30 个 → 解出 30 帧（320x240） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0 | 与软解对比: 30 帧与软解逐像素一致`（P010 读回：color-format 54、stride 640、slice-height 240，解码器 `c2.mtk.hevc.decoder`）。模拟器（15:54:20）同素材同样逐像素一致，但 goldfish 未接住 P010、按设计回退软解：`HEVC 10-bit 自检: 解码器 MediaCodec HEVC | 样本 30 个 → 解出 30 帧（320x240） | 硬件计数 帧 0 会话 1 拒绝 0 回退 1 | 与软解对比: 30 帧与软解逐像素一致`。
- **真机导出验证**：命令面板以显式参数发 `file.exportMedia`（`format:"h264"`、`range:"custom"`、0–2 s、`path:…/exports/export-2s.mp4`、`wait:false`）；logcat：`export: job 4 finished: {"bytes":2975979,"frames":48,"path":"…","render_fps":15.83,"seconds":3.03}`；连续三次运行产物**逐字节相同**（2,975,979 B，sha256 `7f00407754bbdb823c4bb19f46688086a846f986fe11e1771f055e11a0a7d9f5`）；ffprobe 核对：H.264 High 1920×1080 23.976 fps + AAC LC 48 kHz 立体声，encoder tag `FilmCraft 0.2.1`。
- **一句明确**：导出编码器是应用自带的**纯 Rust 编码器**；MediaCodec 硬件编码**没有接**（开放项，见 §6）。

## 4. 构建与部署（可原样照抄）

环境（每个新 shell）：

```bash
export PATH="$HOME/.cargo/bin:$HOME/Library/Android/sdk/platform-tools:$HOME/Library/Android/sdk/emulator:$PATH"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/28.2.13676358"
export JAVA_HOME=/opt/homebrew/opt/openjdk@21
```

`~/.cargo/bin` **必须在最前**（见 §5.1）。默认 shell 里 `adb` 不在 PATH 上。

把补丁镜像进（gitignore 的）克隆——补丁才是源头：

```bash
cd ~/artcraft-mobile/filmcraft
rsync -a ~/artcraft-mobile/patches/filmcraft-android/ apps/filmcraft-android/
rsync -a ~/artcraft-mobile/patches/platform-mediacodec/src/ crates/platform/src/
cp ~/artcraft-mobile/patches/platform-mediacodec/Cargo.toml crates/platform/Cargo.toml
# 可选核对：diff -rq ~/artcraft-mobile/patches/filmcraft-android/src apps/filmcraft-android/src
```

构建 Rust 库（在克隆根目录；release 约 6 分钟，thin LTO、codegen-units 1）：

```bash
cargo +stable ndk -t arm64-v8a -o apps/filmcraft-android/jniLibs build --release -p filmcraft-android
# → apps/filmcraft-android/jniLibs/arm64-v8a/libmain.so（约 46 MB）
```

打包 APK（手动 Gradle——wrapper 不可用，§5.2）：

```bash
cd ~/artcraft-mobile/filmcraft/apps/filmcraft-android/android
~/artcraft-mobile/tools/gradle-9.3.1/bin/gradle assembleRelease --no-daemon -Dhttp.nonProxyHosts='*'
# → app/build/outputs/apk/release/app-release.apk（约 44 MB，debug 签名）
```

安装、启动、看日志（示例为模拟器；其他设备用 `-s SERIAL`）：

```bash
ADB="$ANDROID_HOME/platform-tools/adb"
APK=~/artcraft-mobile/filmcraft/apps/filmcraft-android/android/app/build/outputs/apk/release/app-release.apk
"$ADB" devices -l
"$ADB" -s emulator-5554 install -r "$APK"
"$ADB" -s emulator-5554 shell am start -n ai.storyteller.filmcraft/.MainActivity
"$ADB" -s emulator-5554 logcat | grep -Ei "filmcraft|SafBridge|MediaCodec|wgpu|panic|AndroidRuntime"
```

模拟器（无头 + swiftshader；当前这台就是以 `emulator-5554` 在跑）：

```bash
"$ANDROID_HOME/emulator/emulator" -avd Medium_Phone_API_36.0 \
  -no-window -gpu swiftshader_indirect -no-snapshot -no-boot-anim -no-audio &
```

真机（无线 ADB；**序列号会变**——每次先看 `adb devices -l`；带空格的序列号要加引号）：

```bash
"$ADB" devices -l
# TCP 形态：10.20.163.64:38657
# mDNS 形态带空格/括号："adb-KZDQSSJZW4AURCJN-kXphPk (3)._adb-tls-connect._tcp"
"$ADB" -s "10.20.163.64:38657" install -r "$APK"
"$ADB" -s "10.20.163.64:38657" shell am start -n ai.storyteller.filmcraft/.MainActivity
```

成功的样子（logcat）：`android_main entered` → `hardware decoding: Available("MediaCodec")` → `MediaCodec: decoding 320x240 H.264 with <设备解码器>` → `自检: … 解出 30 帧 … 回退 0` → `touch mode applied` → `loaded system CJK font`。构建验证通过后，把 APK 放进 `dist/`。

## 5. 环境坑与交接要点

1. **Homebrew rust 遮蔽 rustup**：默认 shell 里 `/usr/local/bin/cargo`（Homebrew rust 1.89）在 `~/.cargo/bin` 之前。始终用 `cargo +stable …`，并保证 `~/.cargo/bin` 在 PATH 最前；rustup 默认工具链是 nightly，会踩 MSRV（需 1.95）。
2. **Gradle wrapper 不可用**：`gradlew` 无法下载发行版，因为 `~/.gradle/gradle.properties` 写死代理 `127.0.0.1:7897`（`~/.npmrc` 同）。用手动下载的 `~/artcraft-mobile/tools/gradle-9.3.1/bin/gradle`，并加 `--no-daemon -Dhttp.nonProxyHosts='*'` 完全绕过代理。
3. **模拟器必须无头 + SwiftShader**（`-no-window -gpu swiftshader_indirect …`）：Apple Silicon 上 Vulkan/MoltenVK 不稳，早期探针在模拟器的 `vulkan.ranchu.so` 里 SIGSEGV（逃生口：`WGPU_BACKEND=gl`；FilmCraft 本体后来在模拟器的 Vulkan/SwiftShader 上能稳定跑）。别把长驻模拟器接 `head`（SIGPIPE 会杀掉它）；也别在 cargo 重构建时启动。
4. **`filmcraft/` 是 gitignore 的**——克隆可丢，`patches/` 是记录。每次构建前把补丁镜像进克隆；"我的修改没进 APK"十有八九是镜像过期。
5. **只发 release**：debug `.so` 855 MB；`--release` 是 45–47 MB。release APK 用 debug 签名配置，任何设备都能直接装。
6. **vivo/SAF 选择器 MIME 过滤**：只给 `ACTION_OPEN_DOCUMENT` 传 `*/*` 时 vivo 只显示"文档"、藏掉媒体——`SafBridge` 加了 `EXTRA_MIME_TYPES = {video/*, audio/*, image/*, application/octet-stream}`。
7. **"Recover Unsaved Changes" 弹窗吞触摸**：崩溃/重启后 FilmCraft 会弹恢复对话框，它是模态的、会吃掉触摸——先点掉（Discard / Not Now）再操作，否则点不动（`shots/stage13-palette.png` 里可见它在面板后面）。
8. **JNI 坑**（都已在 `src/saf.rs` 踩过并修好）：用 `vm.attach_current_thread(|env| …)` + jni 0.22（`jni_str!`/`jni_sig!`）；取 Activity 必须用 `AndroidApp::activity_as_ptr()`——`ndk_context` 给的是 *Application*，不能 `startActivityForResult`；原生线程上 `FindClass` 找不到应用类（JNI 规范：系统类加载器），所以 JNI 入口做成 **`MainActivity` 的实例方法**（对象已有、虚调用无需类查找）；大块数据走清单文件（`import-manifest.txt`），不要 JNI 编组。
9. **CJK 字体必须在主题字体生效之后追加**（先确认命名族已存在，再读-改-写 `definitions()`），否则 egui panic：`FontFamily::Name("semibold")` 无字体绑定。
10. **`menus::invoke` 只在没有 paths 时才弹文件对话框**；带 paths 时直接执行引擎命令。所以 SAF 选择结果必须以 `engine.execute`（带 `paths`/`path`）经控制通道投递——这正是桌面 File ▸ Import 在文件已知后走的路径，无需自造 UI。
11. **上游 `AGENTS.md` 规则对所有移植代码有效**（写代码前读 `filmcraft/AGENTS.md`）：永不崩溃（优先级高于功能）；测试之外禁用 `unwrap`/`expect`/`panic!`——未完成的功能返回错误；`unsafe` 只允许在 `crates/platform`（其余 crate 用 `deny` + 入口 `#[allow(unsafe_code)]`，每个块带 `// SAFETY:`，ADR 0001）；所有输入都当敌意处理；素材必须有开放许可 + 署名 sidecar，且 **ffmpeg/ffprobe 只能作测试 oracle / 素材生成器**（绝不链接、不随包发布）。
12. 其他：`pick_folder` 返回应用临时目录（SAF 目录树未接）；模拟器 GPU 上限 4096（桌面/wasm 请求 8192——真机没问题）；手机工作区只在启动时装一次，不要每帧重设。
13. **脚本长按的坐标坑**：在坐标 (540, 40) 长按会连顶部页头的 Export 模式按钮一起命中，偶发切进 Export 工作区——脚本化长按请换一个 x（避开顶栏按钮）。
14. **模拟器 goldfish 曾卡住 P010**：goldfish 接受 P010 configure 后一帧不发（10-bit 自检首启就挂住；重启后 <1 s 内回退软解）。自检没有整段解码的看门狗；未能复现。

## 6. 剩余工作

- 4:2:2 色度——手机硬件不提供（软解回退，已验证），证据见 §2.2；不必实现。
- MediaCodec 编码器未接——导出编码走应用自带的纯 Rust 实现，见 §2.2；需先补编码后端。
- 目录选择器——`pick_folder` 仍写应用自己的 `exports/`；若导出要落用户目录，需接 `ACTION_OPEN_DOCUMENT_TREE`。
- 音频——`cpal`/AAudio 的 `AudioOut` 缝**未接**；Android 上还需要权限与路由（PORTING.md §1 缝合点表）。
- 更多触摸手势——目前只有长按面板 + 40 pt 命中目标；探针已验证 `dragged()` 平移、`zoom_delta()` 双指缩放、`double_clicked()` 复位、`long_touched()` 菜单（EXPERIMENT.md §5.2）——把它们接进时间线/监视器。
- iOS/iPadOS——未动；先做 UIScene 阻断点 spike（PORTING.md §5.1：iOS 27 要求 `UIApplicationSceneManifest`，winit issue #4224；`filmcraft_ui_egui` 需 `default-features=false, features=["default_fonts","wgpu"]`；`rfd` 无 iOS 后端 → `UIDocumentPickerViewController`）。
- 鸿蒙 / HarmonyOS PC——未动；先做 NAPI 实验，再选 ArkWeb（PC）或 `winit-ohos`（PORTING.md §6；注意 `#[cfg(target_env = "ohos")]`、wgpu 只能 GLES 并需 pin git rev）。
- 其余四个应用的分应用说明（macOS 版本已装在 `/Applications`，可供行为对照）：
  - *LightCraft 0.2.1 / PrintCraft 0.2.1*（MSRV 1.90）、*PhotoCraft 0.3.0*、*EffectCraft 0.4.0*（MSRV 1.95）——上游架构相同（engine + ui-egui 应用 crate + `apps/<app>-web` 先例），§7 配方原样适用。
  - 每个应用动手前：grep 其 workspace，确认它实现哪些缝（`HostHooks`、`Services`、`AudioOut/AudioInput`、`platform::register`）——这些就是 Android 外壳要补的全部；再看它怎么用 `crates/platform`（目前只有 FilmCraft 注册视频解码器）以及有无应用专属的平台 FFI（如照片/PDF 管线）。
  - 预期同样有触屏布局缺口与 CJK 字体问题；各应用的 ui-egui crate 有自己的主题/工作区 API——复用模式，不要照搬 filmcraft 的标识符。

## 7. 移植下一个 App（或下一个平台）的方法

从 `patches/filmcraft-android` + `PORTING.md` 提炼。步骤与应用无关；应用相关处会指明该查 PORTING.md §1（缝合点表）与该应用自己的 `AGENTS.md`/`docs/architecture.md`。

1. **克隆上游**到 `~/artcraft-mobile/<app>`（gitignore）。固定 commit 并记录到本文档（FilmCraft：`8fcad73`）。
2. **加入口 crate** `apps/<app>-android`：以 `patches/filmcraft-android` 为模板（Cargo.toml + `src/lib.rs`）；改包名，保留 `[lib] name = "main"`、`crate-type = ["cdylib"]`、`unsafe_code = "deny"` + `android_main` 上的 `#[allow(unsafe_code)]`，以及同一组 eframe features（`default_fonts`、`wgpu`、`android-game-activity`——**accesskit + native-activity 是 eframe 硬 compile error**；面板类应用必须 GameActivity 才有 IME）。依赖该应用自己的 engine/ui-egui crate。
3. **Gradle 外壳**：复制 `patches/filmcraft-android/android/` → `apps/<app>-android/android/`；改 `namespace`/`applicationId`/`android:label` 和 `MainActivity` 包名；保留 `android.app.lib_name=main` 与 `jniLibs.srcDirs = ['../../jniLibs']`（即 `cargo ndk -o …/jniLibs` 的输出目录）。
4. **分层（可选）**：要让 `cargo xtask ci` 通过，在 `xtask/src/main.rs` 的 `LAYERS` 加 `("android", 6)`（克隆里还没加，所以 `cargo xtask ci` 会报未知层；不影响构建）。
5. **实现宿主缝**（按 PORTING.md §1 的清单）：所有文件对话框 → `HostHooks`（异步选择器，结果以带 paths 的 `engine.execute` 走控制通道——见第 10 条）；`Services`/数据目录 → `AndroidApp::internal_data_path()`（自动保存、崩溃日志、偏好）；有音频的应用 → cpal 的 Android 后端；有视频解码的 → `platform::register()`。
6. **平台原生码**只放进 `crates/platform` 风格的 crate：`unsafe_code = "deny"`、FFI 模块 `#[allow(unsafe_code)]`、处处 `// SAFETY:`、安全的 `Result` API、保留上游回退机制（`HybridDecoder` 模式），要合上游就同步修订 ADR 0001。Android 上 `ndk` crate 的安全 MediaCodec 绑定覆盖了解码，只花了一个 `Send` 实现；照此评估应用自身需求（采集？编码？）。
7. **触摸外壳 + 手机工作区**：照 TouchShell 的做法——**第一帧之后**再调参（注意主题字体时序！）、40 pt 命中目标、不做缩放；长按面板映射到该应用自己的命令 id（先核对 `engine.execute` 命令表）；经应用自己的工作区 API（FilmCraft 是 `set_workspaces`）装手机布局。
8. **自检**：把一段 ffmpeg 生成的小片（`assets/test.*`，`include_bytes!`）内嵌，启动时经引擎自己的工厂路径解码，logcat 打印解码器名、帧数、hw 计数。这正是 MediaCodec 后端得到证明的方式（与解码无关的场景可换成平台自己的自检）。
9. **构建与验证**用 §4 的命令块；模拟器 *和* 真机都装；截图存 `shots/`、阶段记录写进 `EXPERIMENT.md`；APK 发布到 `dist/`。
10. **纪律**：只改 `patches/`，再镜像进克隆；用 `shots/` 与文档维护证据链；不要提交 `filmcraft/`、`.so`、`jniLibs/`、`build/`、`tools/`。

同一配方在别的平台上的差异（细节见 PORTING.md §5/§6）：iOS 需要 `staticlib` 入口 + Xcode 工程 + UIScene 规避 + `UIDocumentPickerViewController` 桥；鸿蒙需要 ArkTS 外壳（NAPI 或 ArkWeb）以及用 OHOS AVCodec 取代 MediaCodec。无论哪种，变的只有四件事：入口 crate、文件选择桥、编解码后端、触摸外壳——缝在上游都已备好。

## 8. 索引

| 读什么 | 得到什么 |
|---|---|
| `PORTING.md` | 分析：路线 A/B/C、各平台步骤、各应用缝合点表（§1）、Android 细节（§4）、风险（§9） |
| `EXPERIMENT.md` | 逐阶段证据、logcat 摘录、环境坑表（§"环境踩坑总汇"）；**§5.10 = 10-bit/导出验证** |
| `patches/filmcraft-android/README.md` | 交付物 1：如何放进仓库、Gradle 外壳、已知缺口 |
| `patches/platform-mediacodec/README.md` | 交付物 2：MediaCodec 后端设计、验证、限制 |
| `shots/` | 截图（`stage13-palette.png` = 命令面板；`stage12-real-*.png` = 真机导入/工作区；`stage5-decode-*.png`、`stage3-*.png` = 探针） |
| `filmcraft/AGENTS.md`、`filmcraft/docs/architecture.md`、`filmcraft/docs/control-protocol.md` | 上游规则、分层、各移植共用的命令/控制通道接口 |
| `STORAGE.md` | 本机磁盘预算（哪些构建树可删） |
| `dist/` | 发布 APK（本交接后创建） |
