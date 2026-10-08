# photocraft-android：把 PhotoCraft 接到 Android

上游没有任何移动端代码。这里给出「加一个平台 crate」的最小集成路径（与上游 `apps/photocraft-web` 同形：都在 `apps/` 下、都只依赖 engine + ui-egui，不依赖桌面入口 crate `apps/photocraft`）。

PhotoCraft 与 FilmCraft 的关键差别：**PhotoCraft 的宿主缝不是 `HostHooks`，而是 `photocraft_ui_egui::Services`**（ui-egui 自己不碰 I/O，所有文件对话框/编解码/偏好都由 app crate 注入）。本 crate 填的就是这个结构体，另外 PhotoCraft 没有视频管线，所以**没有** `platform::register` / MediaCodec 这一层。

## 1. 放进仓库

```bash
cp -R patches/photocraft-android <photocraft-clone>/apps/photocraft-android
```

无需改根 `Cargo.toml`：workspace 成员是 `apps/*` 通配（`default-members` 不含它，所以 `cargo build` 默认不会构建它；用 `-p photocraft-android` 显式构建）。

跑 `cargo xtask ci` 的话，在 `xtask/src/main.rs` 的 `LAYERS` 里加 `("android", 6)`（只影响 xtask，不影响构建）。

## 2. 填哪些缝（`Services`）

| 桌面（`apps/photocraft/src/services.rs`） | Android（`src/services.rs`） |
|---|---|
| `rfd` 文件对话框 | SAF 选择器（`src/saf.rs` + Java `SafBridge`）：选中的文件复制进 `<filesDir>/import/`，路径经 `OsEvent::Open` 回到 app |
| `photocraft_io::import` / `export` | 同一对函数（引擎自己的解码/编码路径），外面套 `crash_guard` 同形的 `catch_unwind` |
| `config_dir()`（`app_dirs`） | `AndroidApp::internal_data_path()`（`<filesDir>/{config,Recovery,import,export}`） |
| `write_atomic` | `photocraft_format::atomic_write`，并在文档来自 SAF 时**回写**原 `content://` URI |
| `arboard` 剪贴板 | 未接（桌面 crate 的 arboard 没有 Android 后端；会话内剪贴板仍可用） |
| `open::that` | 未接（`ctx.open_url` 覆盖 About 里的链接） |
| `read_displays`（显示器 ICC） | 未接（画布用 Color Settings 里的配置文件或 sRGB） |
| `preset_store`（画笔预设库） | 未接（预设仅本次会话，与 web 构建一致） |

`Services::pick_open` / `pick_open_paths` 是**同步**回调（要在当帧回答「选了哪些文件」），而选择器无法同步返回，所以两个 hook 都**立刻返回「没有文件」**并在后台起选择器；选中的路径稍后经 app **自己的** `os_events` 缝（`OsEvent::Open`，桌面端是 Finder 双击/Open With 用的那条）进入 `open_paths`——名字、记住的路径、Open Recent、告警全部走 app 原有逻辑，本 crate 不碰任何 UI 代码。

## 3. Gradle 外壳

`patches/photocraft-android/android/` 是一份可直接用的 Gradle 工程（AGP 9.1.0 / Gradle 9.3.1 / JDK 21、minSdk 31 / target 35、`androidx.games:games-activity:4.4.0`）：

| 位置 | 值 |
|---|---|
| `app/build.gradle` → `namespace` / `applicationId` | `ai.storyteller.photocraft` |
| `app/build.gradle` → `versionName` | `0.3.0` |
| `app/src/main/AndroidManifest.xml` → `android:label` | `PhotoCraft` |
| `android.app.lib_name` | `main`（与本 crate 的 `[lib] name = "main"` 一致） |
| `jniLibs.srcDirs` | `['../../jniLibs']`（cargo-ndk 的输出目录） |

必须用 **GameActivity**，不能用 NativeActivity：`accesskit`（ui-egui 的 eframe 依赖里写死了这个 feature）+ `android-native-activity` 是 eframe 的硬 `compile_error!`；`android-game-activity` 则没问题，而且面板类应用需要它的 IME。

图标沿用 Android Studio 的占位 launcher 图标（与 `patches/filmcraft-android/android/app/src/main/res/mipmap-*` 同一套，纯装饰、可替换）：ArtCraft 的品牌图形在 `docs/brand/` 里且**不是**开源素材（`docs/brand/LICENSE-brand.txt`），所以移植包不引用它们。

## 4. 构建（本机实测命令）

```bash
export PATH="$HOME/.cargo/bin:$HOME/Library/Android/sdk/platform-tools:$PATH"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/28.2.13676358"
export JAVA_HOME=/opt/homebrew/opt/openjdk@21

cd <photocraft-clone>
cargo +stable ndk -t arm64-v8a -o apps/photocraft-android/jniLibs build --release -p photocraft-android
# → apps/photocraft-android/jniLibs/arm64-v8a/libmain.so（49,627,400 B ≈ 47.3 MB）

cd apps/photocraft-android/android
~/artcraft-mobile/tools/gradle-9.3.1/bin/gradle assembleRelease --no-daemon -Dhttp.nonProxyHosts='*'
# → app/build/outputs/apk/release/app-release.apk（47,014,735 B ≈ 44.8 MB，debug 签名）

adb install -r app/build/outputs/apk/release/app-release.apk
adb shell am start -n ai.storyteller.photocraft/.MainActivity
adb logcat | grep -Ei "photocraft|panic|AndroidRuntime|wgpu|SafBridge"
```

`--release` 必须（debug `.so` 数百 MB）。PhotoCraft 的 release profile 没有设 `debug = ...`，所以 `--release` 就是对的（lightcraft 那种设了 `debug = "line-tables-only"` 的才要自己的 `dist` profile）。

## 5. 已验证（模拟器 `Medium_Phone_API_36.0`，无头 + SwiftShader；2026-10-09）

产物：`dist/PhotoCraft-0.3.0-android-arm64.apk`，sha256 `080bf4496ab8b78d434cec005524dbc940b71b045eba134d0811436704596780`；**模拟器上安装并运行的那个 APK 与本文件逐字节相同**（`adb pull $(pm path …)` 后比对 sha256）。

logcat（启动）：

```
photocraft-android: android_main entered
photocraft-android: data directory /data/user/0/ai.storyteller.photocraft/files
egui_wgpu: There are 2 available wgpu adapters: {backend: Vulkan, device_type: Cpu, name: "SwiftShader Device …"}
photocraft_ui_egui::gpu_canvas: gpu canvas: target Rgba8Unorm, max texture 8192, tile 8192, 16F canvas Budget
photocraft-android: touch mode applied (ppp 2.625, logical 411×914 pt, interact 40 pt)
photocraft-android: self-test: 3 document(s), PSD 862 B → 16×16 px #ff0000 ✓ | PNG 109 B → 16×16 px #ff0000 ✓
photocraft-android: loaded system CJK font /system/fonts/NotoSansCJK-Regular.ttc
```

无 `FATAL` / `AndroidRuntime` / `panicked` 行；Activity 首帧 `Displayed ai.storyteller.photocraft/.MainActivity for user 0: +1s250ms`。GPU 画布（自定义 WGSL）在 SwiftShader 上初始化成功（真机驱动失败时 `set_wgpu` 内部会 `catch_unwind` 退回 CPU 画布，不会崩）。

**启动自检**（`src/selftest.rs`，后台线程）：用本 shell 注入的同一套 `Services` 跑一次 app 自己的 `file.new → edit.fill → save_as(PSD) → save_as(PNG) → open_path → 取像素`，即 `services.import` / `export` / `write` 三个缝在设备上的实证（不内嵌任何素材，文档由引擎自己合成，无授权问题）。

**照片打开（关键路径，实测通过）**：

```
SafBridge: copied content://com.android.externalstorage.documents/document/primary%3APictures%2Fphotocraft-test.png
           -> /data/user/0/ai.storyteller.photocraft/files/import/photocraft-test.png
main::saf: SAF: 1 file(s) picked
main::services: SAF: 1 file(s) picked → opening
```

截图 `shots/photocraft-emulator-open.png`：文档标签 `photocraft-test.png @ 100% (RGB/8)`、画布上是那张 256×256 渐变图、状态栏 `256 px x 256 px (72 ppi)`。

**保存/另存为（关键路径，实测通过）**——命令面板 ▸ 另存为…（`ui.menu.invoke file.saveAs`）：

```
photocraft-android: save dialog → /data/user/0/ai.storyteller.photocraft/files/export/photocraft-test.png
palette: file.saveAs replied {"ok":true,"result":{"path":"…/export/photocraft-test.png","warnings":[]}}
SafBridge: save destination: content://com.android.externalstorage.documents/document/primary%3APictures%2Fphotocraft-saved.png
SafBridge: published …/export/photocraft-test.png -> content://…photocraft-saved.png
SAF: published …/export/photocraft-test.png (780 bytes) to the chosen destination
```

拉回来核对：`/sdcard/Pictures/photocraft-saved.png` = 780 B，`file` 认出 **PNG image data, 256 x 256, 8-bit/color RGB, non-interlaced**（引擎自己编码的成品，落在用户选的位置）。取消时文件留在 `filesDir/export/`，不丢东西。

## 6. 触屏外壳

- **不缩放**（`TOUCH_SCALE = 1.0`，与 FilmCraft 试点同一结论）：手机逻辑视口只有 411×914 pt，放大只会更挤。改为 40 pt 命中目标、更宽的 `slider_width`、手指停住才显示 tooltip（第一帧之后设置，即 PhotoCraft 装完自己的主题之后）。
- **手机布局＝画布优先**（`phone_layout`）：右侧停靠栏最小宽度是 `panels::DOCK_WIDTH` 的 250 pt，而手机只有 411 pt——留着它画布只剩约 110 pt。所以启动时隐藏 Color/Properties/Layers 三组（工具条 + 选项栏 + 画布占满屏宽），三块面板放进命令面板，其余面板用 app 自带的 `Edit ▸ Search…` 搜；用户重新打开的面板会被 app 自己记住。
- **命令面板**：长按左上角 PhotoCraft 图标 500 ms（该处只有 `Sense::hover()` 的品牌标记，不会误触菜单；FilmCraft 试点踩过「长按顺带点到顶部按钮」的坑）。面板按钮全部是 **PhotoCraft 自己的命令 id**，经控制通道以菜单语义执行（`ui.menu.invoke`），行为与点菜单一致：`file.open` / `file.new` / `file.save` / `file.saveAs` / `file.export.quickExportAsPng` / `file.export.exportAs` / `file.close` / `edit.search`，加 `window.panel.{layers,color,properties}` 三块面板开关。实测：

  ```
  main: long-press on the title strip → command palette
  main: palette: 打开… → ui.menu.invoke file.open
  main: palette: file.open replied {"ok":true,"result":null}
  main: palette: 面板：图层 → ui.menu.invoke window.panel.layers
  main: palette: window.panel.layers replied {"ok":true,"result":{"panel":"layers","tab":0,"visible":true}}
  ```

  截图：`shots/photocraft-emulator.png`（画布优先首屏）、`shots/photocraft-emulator-open.png`（打开照片）、`shots/photocraft-emulator-layers.png`（面板开关生效，Layers 里能看到 Background 层缩略图）。
- **CJK 字体**：`/system/fonts/NotoSansCJK-Regular.ttc` 在**主题字体生效之后**读-改-写 `definitions()`（追加到**所有**族，包括主题的 `medium`/`semibold` 命名族——否则 egui 会因命名族没有字体绑定而 panic）；Preferences 改主题会重建字体，所以每帧只做一次廉价的「还在不在」检查，缺了就补。截图里的中文按钮即证明。

## 7. 已知缺口（首版）

- **剪贴板**：`arboard` 没有 Android 后端，`clipboard_get/set_image` 未接——Edit ▸ Copy/Paste 只在应用内部生效，与系统剪贴板不通。
- **`open_url`** 未接（About 里的链接点了没反应；`ctx.open_url` 是 web 路径）。
- **后台任务**：`background_jobs` 保持 false（桌面默认打开）。所以打开大文件会短暂占用 UI 线程；要接的话得先确认 job 线程在 Android 上没问题。
- **单文件选择类命令**（Place Embedded、载入画笔/渐变、脚本等走 `pick_file_bytes` 的命令）：hook 无法同步返回字节，会退化成「把选中的文件作为新文档打开」；多文件 File ▸ Open 是正常路径。
- **保存回原文件**：`ACTION_OPEN_DOCUMENT` 带 `FLAG_GRANT_WRITE_URI_PERMISSION`，File ▸ Save 会回写原 `content://`（`safPublishTo`）；只读提供者（如 Google Photos 分享的文档）会失败并只留警告，此时用另存为。
- **文件夹选择器**：PhotoCraft 的 `Services` 没有 `pick_folder`（桌面用 rfd 的地方只有上面这些），所以无需实现。
- **HEIF/HEIC**：上游是可选 feature（`photocraft-codecs/heif`），本构建未开。
- **手写笔压感**：`crates/tablet` 只有 macOS AppKit 与 X11 XInput2 两条路径，Android 未接（winit 在 Android 上不送压感）。
- **MediaCodec**：PhotoCraft 没有视频管线，`crates/platform` 的硬解后端与它无关；导出编码走引擎自带的纯 Rust 编码器。
- **`unsafe_code` 政策**：本 crate 用 `deny` + JNI/入口处 `#[allow(unsafe_code)]`（`#[unsafe(no_mangle)]` 与 `JObject::from_raw` 需要），每个块带 `SAFETY:` 说明，与 `crates/platform`、`patches/filmcraft-android` 的既有先例一致。

## 8. 与 FilmCraft 试点的差异（给下一个 app）

1. **缝不同**：FilmCraft 是 `HostHooks`（引擎侧 trait），PhotoCraft 是 `Services`（ui-egui 侧结构体，含 import/export/prefs/recovery/剪贴板）。动手前先 grep 该 app 的 ui-egui 依赖了哪些桌面 crate。
2. **异步结果投递不同**：FilmCraft 用控制通道 `engine.execute{"command":"file.import","params":{"paths":[…]}}`；PhotoCraft 没有 `file.import`，它自己的异步缝是 `Services::os_events`（`OsEvent::Open`），走 app 的 `open_paths`。控制通道在这里用于**触屏命令面板**（`ui.menu.invoke`）。
3. **PhotoCraft 自带命令面板**（`Edit ▸ Search…`，Cmd+K，560 pt 宽），手机上偏宽，所以外壳自己做了窄面板；不要重复造搜索。
4. `ui-egui` 里**没有** `rfd`/`arboard`（只有桌面入口 crate 有），所以 Android 构建不会撞上 PrintCraft 那种硬编译错误。
