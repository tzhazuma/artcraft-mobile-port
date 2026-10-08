# FilmCraft → Android 实验记录

> 环境：macOS 27.2/arm64（M3 Pro），Rust stable 1.99.0，Android SDK `~/Library/Android/sdk`（AVD `Medium_Phone_API_36.0`，arm64-v8a，Android 36，Play 镜像，Chrome 133），NDK r28.2.13676358，cargo-ndk 4.1.2，JDK 27（系统）/ 21（Homebrew，Gradle 用它），Gradle 9.3.1 + AGP 9.1.0。

## 结果速览

| 阶段 | 结果 |
|---|---|
| 0. wasm 版在 Android 跑通 | ⚠️ 加载链路全通（HTTP 200 / wasm 执行），但模拟器 GPU 上限 4096 < 应用请求 8192，启动被 wgpu 拒绝 —— 换真机大概率可过 |
| 1. Android 工具链 | ✅ Rust stable 1.99 + NDK r28.2 + cargo-ndk 4.1.2 + Gradle 9.3.1 |
| 2/4. FilmCraft 交叉编译 | ✅ **完整技术栈编译 + 链接通过**（engine/ui-egui/wgpu/winit/GameActivity），产出 `libmain.so`（debug 855 MB） |
| 3. 最小 eframe+wgpu Android 宿主 | ✅ **APK 打包 + 在模拟器上跑起来**（截图 `shots/stage3-2.png`）：egui 界面正常渲染、frame 计数递增；`zipalign -c -P 16` 通过、`.so` LOAD 段 `0x4000` 对齐（满足 16 KB 页政策） |
| 待办 | FilmCraft 本体出 APK 并用真机复测（模拟器的 Vulkan 驱动会崩、GPU 上限 4096，见「踩坑」） |

## 阶段 1：Android 工具链 ✅

| 项 | 状态 | 备注 |
|---|---|---|
| Rust stable ≥1.95 | ✅ 1.99.0 | 首次 `rustup toolchain install stable` 因残留状态失败，重试成功 |
| Android targets | ✅ | `aarch64-linux-android`、`x86_64-linux-android` |
| cargo-ndk | ✅ 4.1.2 | `cargo install cargo-ndk --locked` |
| NDK r28.2.13676358 | ✅ | 原机只有 r26（16 KB 页未对齐）；用 `sdkmanager --sdk_root=$HOME/Library/Android/sdk` 安装 |
| Java / adb / AVD / Gradle | ✅ | adb 36.0.1；Gradle 9.3.1 手动下载解压（wrapper 走不通，见「踩坑」） |

## 阶段 0：wasm 版在 Android —— 加载成功，GPU 初始化被模拟器限制挡住

做法：官方 `filmcraft-web-0.2.1.zip`（10.9 MiB）→ `python3 -m http.server 8765` → `adb reverse tcp:8765 tcp:8765` → 模拟器 Chrome 打开 `http://localhost:8765/`。

结果（截图见 `shots/stage0-*.png`）：

1. ✅ 页面、`filmcraft_web.js`、`filmcraft_web_bg.wasm`（35 MB）经本地服务器全部 200 加载成功，wasm 已开始执行。
2. ❌ FilmCraft 启动失败，页面显示：

   ```
   FilmCraft failed to start:
   Limit 'max_texture_dimension_2d' value 8192 is better than allowed 4096
   ```

含义：wasm 在 Android 的**加载链路（本地安全上下文、静态服务、wasm 编译执行）没有问题**；卡点在 **GPU limits 协商**，是模拟器环境差异（真实手机普遍 ≥8192）。这正是「先用最便宜的方式验证可行性」要抓的那类结论。

## 阶段 2 / 4：filmcraft-android 交叉编译 + 链接 —— ✅ 通过（关键里程碑）

新增 `apps/filmcraft-android`（见 `patches/filmcraft-android/`）：`cdylib` + `android_main` + 复用 `Session`/`FilmcraftApp`，数据目录接 `AndroidApp::internal_data_path()`，`HostHooks::default()`（文件选择器留待 SAF 桥接）。

```bash
cargo +stable ndk -t arm64-v8a check -p filmcraft-android      # 28s，全部通过
cargo +stable ndk -t arm64-v8a -o apps/filmcraft-android/jniLibs build -p filmcraft-android   # 2m08s
# → apps/filmcraft-android/jniLibs/arm64-v8a/libmain.so（debug 855 MB；APK 内 strip 后 27 MB）
```

**结论：FilmCraft 的完整技术栈（引擎 + egui UI + wgpu + winit + android-activity/GameActivity）可以整体交叉编译并链接到 `aarch64-linux-android`**；新 crate 用 `unsafe_code = "deny"` + 入口 `#[allow(unsafe_code)]`，与上游 `crates/platform` 的既有先例一致。

## 阶段 3：最小 eframe+wgpu Android 宿主（probe）—— ✅ APK 打包成功

工程 `probe/egui-android-probe/`：eframe 0.36（`default-features=false` + `wgpu` + `android-game-activity`）、winit 0.30、GameActivity + Gradle 骨架（拷自 `rust-mobile/android-activity` 的 `agdk-mainloop`）。

```bash
cargo +stable ndk -t arm64-v8a -o app/src/main/jniLibs build     # ✅ libmain.so
gradle assembleDebug -Dhttp.nonProxyHosts='*'                    # ✅ BUILD SUCCESSFUL in 53s
# → app/build/outputs/apk/debug/app-debug.apk（34 MB）
zipalign -c -P 16 -v 4 app-debug.apk                             # ✅ Verification successful
llvm-readelf -lW lib/arm64-v8a/libmain.so | grep LOAD            # ✅ Align=0x4000（16 KB）
```

关键 API 修正：**eframe 0.36 的 `App` trait 是 `fn logic(&Context)` + `fn ui(&mut Ui, &mut Frame)` 两段式**，旧教程里的 `fn update(&mut self, ctx, frame)` 已不存在（编译报 `not a member of trait`）。

### 在模拟器上真跑起来（截图 `shots/stage3-2.png`）

```text
egui + wgpu on Android
──────────────────────
frame: 124
FilmCraft □□□□□ eframe 0.36 + wgpu + winit 0.30 + GameActivity
```

- 日志证明全链路：`GameActivity: Found library libmain.so` → `main: egui-android-probe: android_main entered` → `egui_wgpu: Software rasterizer detected`（ANGLE/SwiftShader）。
- **第一次启动 SIGSEGV**：backtrace 顶层是 `/vendor/lib64/hw/vulkan.ranchu.so (vk_common_SetDebugUtilsObjectNameEXT+256)` —— 崩在**模拟器的 Vulkan 驱动**里，不是应用代码；wgpu 当时枚举到 2 个适配器（Vulkan/SwiftShader 与 GL/ANGLE）。
- **解法**：启动时 `std::env::set_var("WGPU_BACKEND", "gl")` 强制 GLES 后端 → 一次通过，界面正常渲染（软件光栅化约 4 fps，真机 GPU 会快得多）。
- 顺带发现：默认字体不含中文（截图中方块），这正是上游用 `craft-fonts` 解决的那类问题，移动端同样要处理。

## 阶段 5：release 构建 + 触屏 UI + MediaCodec 硬解（本轮）

### 5.1 release 体积（debug vs release）

| 产物 | debug | release |
|---|---|---|
| probe `libmain.so` | 237 MB | **16 MB** |
| probe APK | 34 MB | **17 MB** |
| FilmCraft `libmain.so` | 855 MB | **45 MB** |
| FilmCraft APK | —（未出包） | **42 MB** |

release 构建：`cargo +stable ndk -t arm64-v8a -o <jniLibs> build --release` + `gradle assembleRelease`（本仓库用 debug 签名以方便安装）。构建耗时：probe 约 1 分钟（依赖已预热）、FilmCraft 约 6 分钟（`lto = "thin"`、`codegen-units = 1`）。

### 5.2 触屏 UI 探针（截图 `shots/stage5-touch-cjk.png`）

`probe/egui-android-probe` 重写为触屏优先布局：顶部状态栏、可平移/缩放的画布、手势日志、大尺寸（≥48dp）底部工具条。手势用 egui 的内建能力：

| 手势 | API | 实测 |
|---|---|---|
| 拖动平移 | `resp.dragged()` + `drag_delta()` | ✅ |
| 双指缩放 | `ctx.input(|i| i.zoom_delta())` / `multi_touch().num_touches` | ✅（日志显示「多指接触：N 指」） |
| 双击复位 | `resp.double_clicked()` | ✅ |
| 长按菜单 | `resp.long_touched()` | ✅（egui 0.36 直接支持） |

**中文字体**：egui 自带字体不含 CJK，默认渲染成方块 —— 运行时读设备字体即可解决（`/system/fonts/NotoSansCJK-Regular.ttc`，`FontData::from_owned` + 挂到 `families` 尾部做回退；`.ttc` 靠 `FontData.index` 选面）。这对上游的意义：移动端要么随包带 craft-fonts，要么走系统字体回退。

### 5.3 MediaCodec 硬解（截图 `shots/stage5-decode-*.png`）

`src/mediacodec.rs` 用 `ndk` crate 的 AMediaCodec 绑定（`features = ["media","api-level-31"]`）解码一段内嵌的 H.264 Annex-B 测试流（ffmpeg 生成：320×240、30fps、1s，9.7 KB，`include_bytes!` 打包）。

实测（模拟器）：**解码器 `c2.goldfish.h264.decoder`；创建 ✓ / configure ✓ / start ✓；输出 320×240**（logcat: `CCodecBuffers: ... width: 320, height: 240`）。

关键发现（两轮对照）：
- **整段 Annex-B 塞进一个输入缓冲 → 只解出 1 帧**（`送入 9957 字节，解出 1 帧`）；
- 改成**按访问单元（AU）逐帧送**（扫描起始码、遇到新的 VCL NAL 就切一刀）→ **解出 16 帧**（同一段 30 帧的流；模拟器解码器 + 15s 送流窗口，未全部解完）。

结论：MediaCodec 的 FFI 路径（AMediaCodec + AMediaFormat）在 Android 上可用；生产接入（上游 `crates/platform`）必须按 AU 逐帧送，而不是整段灌。

### 5.4 🎉 FilmCraft 本体在 Android 上跑起来（截图 `shots/stage6-*.png`）

```bash
cargo +stable ndk -t arm64-v8a -o apps/filmcraft-android/jniLibs build --release -p filmcraft-android   # 6m04s
cd apps/filmcraft-android/android && gradle assembleRelease          # 42 MB APK
adb install -r app/build/outputs/apk/release/app-release.apk
adb shell am start -n ai.storyteller.filmcraft/.MainActivity
```

结果：
- **完整桌面 UI 在手机上渲染**：菜单栏、Properties（"Select a clip to see its properties"）、Project 面板、时间线、音频表（dB）、状态栏提示；
- 点击 **Open Demo Project 后工程加载成功**：Project 面板列出 `Footage 6 items` / `Audio 1 item`，Timeline 出现 `V1/V2/A1/A2` 轨道、蓝色片段块、时间码 `00:00:04:0`、走带控制；
- wgpu 在模拟器上选了 **Vulkan（SwiftShader）** 后端并稳定运行（未复现探针早期的 Vulkan 崩溃）；
- 数据目录走 `AndroidApp::internal_data_path()`（自动保存/偏好），日志显示 `Installing profile for ai.storyteller.filmcraft`。

同时也暴露了预期中的问题：**桌面布局直接搬到手机上不可用** —— 菜单栏文字互相重叠、命中目标只有几像素、时间线被挤成一条窄缝。这正是「触屏 UI」要做的工作（探针 5.2 演示了可行的一组做法）。

### 5.5 触屏适配实测（FilmCraft 本体，`patches/filmcraft-android`）

在 Android 外壳里加了一个 `TouchShell`：转发 `FilmcraftApp` 的 `logic`/`ui`，在**第一帧之后**（即 FilmCraft 自己的主题装完之后）调整 egui 的缩放与间距。实测结论有两条：

1. **先量尺寸再改**：手机（1080×2400 @420dpi）上 egui 的逻辑视口只有 **411×914 pt**（`pixels_per_point = 2.625`），而桌面布局是按 ≥900×560 pt 设计的 —— 也就是说手机上横向只有设计宽度的一半。
2. **单纯放大反而更糟**：试过 `pixels_per_point × 1.35`（截图 `shots/stage7-filmcraft-touch.png`），逻辑视口缩到约 305×677 pt，面板互相重叠、时间线被压成一条 —— 所以 `TOUCH_SCALE` 定回 **1.0**，只保留「放大命中目标」的部分：`interact_size = 40pt`（约 105 px 物理尺寸）、`button_padding`、`item_spacing`、以及「手指停住才显示 tooltip」。

**真正的触屏 UI 不是缩放问题，是布局问题** —— 已用上游自己的 workspace API 落地了第一步（**不改 upstream 代码**）：
Android 外壳在启动时通过 `FilmcraftApp::set_workspaces` 安装一个 `Phone` 工作区
（`DockNode::Split { vertical: true, Ratio(0.42), Program / Tabs[Timeline, Project, Tools] }`），
手机上一屏就是「监视器 + 时间线」，项目箱与工具收进底部标签页（截图 `shots/stage11-phone-workspace2.png`）。
后续可继续做：把顶部菜单换成手势 + 长按菜单、把 143 个快捷键映射成触屏操作。

顺带验证：**系统 CJK 字体加载生效**（logcat: `loaded system CJK font /system/fonts/NotoSansCJK-Regular.ttc`）。

### 5.6 硬解落地：`crates/platform` 的 MediaCodec 后端（✅ 端到端验证）

`patches/platform-mediacodec/`（覆盖 `crates/platform/` 的三个文件）给 Android 接上了真正的硬解后端，
接法与 macOS 的 VideoToolbox 完全同形：

- `register()` 在 Android 上返回 `Available("MediaCodec")` 并注册 `mediacodec_factory`；
- 工厂返回 `HybridDecoder::new(Box::new(MediaCodecDecoder), entry, info)` —— 中途失败自动切软解、计数走
  `note_hw_fallback()`，全部复用上游现成机制；
- `MediaCodecDecoder`：avcC→Annex-B、csd-0/1 用 SPS/PPS 配置、输出按 `color-format`/`stride`/`crop`
  转成与软解一致的 `PixelData::Yuv8` 三平面（NV12 顺手去交织）、EOS 用带 flag 的空输入缓冲排空；
- 拒绝策略：非 H.264 / >8bit / 非 4:2:0 / 隔行 / 设备只有软件解码器（`c2.android.*`、`*.sw.*`）→
  交回软解并 `note_hw_declined()`；
- 唯一的 `unsafe` 是 `SendCodec` 的 `unsafe impl Send`（`AMediaCodec` 无线程亲和性，`&mut self` 保证不并发），
  按 ADR 0001 放在 FFI 模块并带 `// SAFETY:` 说明。

**验证**（FilmCraft release APK 启动时的内嵌片段自检，logcat）：

```
I filmcraft_platform::mediacodec: MediaCodec: decoding 320x240 H.264 with c2.goldfish.h264.decoder
I filmcraft_platform::mediacodec: MediaCodec: output color-format 0x15, stride 320, slice-height 240, picture 319x239 at (0,0)
I main: filmcraft-android: 自检: 解码器 MediaCodec H.264 | 样本 30 个 → 解出 30 帧（319x239） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0
```

即：**引擎自己的 `make_video_decoder()` 路径**（注册工厂 → HybridDecoder → MediaCodecDecoder）
把 30 个样本全部解出，`hw_stats` 同步增长，无回退。自检代码在
`patches/filmcraft-android/src/selftest.rs`（解析内嵌 Annex-B → 构造 `avc1` SampleEntry →
逐个访问单元送解码器 → 报告解码器名/帧数/计数器）。自检覆盖 **H.264 与 HEVC 两段片段**
（HEVC 的 `csd-0` 要打包 VPS+SPS+PPS，NAL 类型在首个字节高 6 位）。

### 5.7 SAF 文件导入（接进 FilmCraft 自带的 Import，模拟器端全链路验证）

**设计**（`patches/filmcraft-android/src/{lib.rs,saf.rs}` + `android/.../SafBridge.java` + `MainActivity`）：

- 入口就是 **FilmCraft 自己的 File ▸ Import… / Open Project…**：`HostHooks` 是同步回调（当帧就要回答
  「选了哪些文件」），所以 hook 里**异步启动** SAF 选择器并立即返回「没有文件」；用户选完后，复制进
  应用私有目录的路径经**控制通道**投递为
  `engine.execute{"command":"file.import","params":{"paths":[…]}}` —— `menus::invoke` 只在**不带 paths**
  时才弹对话框，带 paths 时直接执行引擎命令，**即桌面端 Import 走的同一条命令**，没有任何自造的导入按钮；
- 后台线程经 JNI 调 `MainActivity.safPickMedia()` → `ACTION_OPEN_DOCUMENT` → 用户选完由
  `onActivityResult` 把文件**复制进应用私有目录**（引擎的文件接口只认真实路径，`content://` 用不了），
  路径写进 `<filesDir>/import-manifest.txt`，Rust 轮询 `safIsDone()` 后读清单；
- 模拟器实测（logcat，`shots/stage10-*.png`）：
  ```
  SafBridge: copied content://...saf-test.mp4 -> /data/user/0/ai.storyteller.filmcraft/files/import/3_saf-test.mp4
  SAF: 1 file(s) picked
  SAF: file.import replied {"ok":true,"result":{"errors":[],"items":[1]}}
  ```
  随后 Project 面板出现该素材（`Search 1 item` + 缩略图）。
- 三个 JNI 坑（都已修）：① `ndk_context` 给的是 **Application** 不是 Activity（要用
  `AndroidApp::activity_as_ptr()`）；② 原生线程 `FindClass` 找不到应用类（JNI 规范：用系统类加载器），
  所以入口放在 **MainActivity 的实例方法**上（对象已有、虚调用无需 FindClass）；③ jni 0.22 的
  `attach_current_thread(|env| ...)` + `jni_str!`/`jni_sig!` 与旧版写法不同；大数组/字符串回传改用
  **清单文件**，JNI 面只剩两次调用。
- 字体时序：主题在第一帧安装并替换字体，CJK 回退必须**等主题字体生效后**再追加（`ctx.fonts(|f| f.definitions().clone())`
  读-改-写），否则 egui 会因 `FontFamily::Name("semibold")` 无绑定而 panic。

**真机验证**（vivo PA2573 / Android 16 / arm64，无线调试）：

| 项 | 结果 |
|---|---|
| 安装与启动 | ✅ 42 MB release APK 直接装、正常启动、完整桌面 UI 渲染（`shots/stage9-real-1.png`） |
| **硬解** | ✅ 用**厂商硬件解码器**：H.264 `c2.mtk.avc.decoder`、HEVC `c2.mtk.hevc.decoder`，各 30 样本 → 30 帧，计数 30/1/0/0，无回退 |
| 触摸输入 | ✅ **可用**（先前误判）：`input:` 日志显示 winit/egui 收到 `Touch` 事件且坐标换算正确（像素 ÷ 2.5 = pt）；当时"点不动"是因为按横屏截图估的坐标、而设备方向在变，加上 `dumpsys gfxinfo` 不统计 SurfaceView 的 GL 帧。修正坐标后：File 菜单能打开、系统选择器能唤起 |
| SAF 导入 | ✅ **真机全链路通过**：`File ▸ Import…` → vivo 文件选择器 → 选中 `saf-test.mp4` → Project 面板出现 `1_saf-test.mp4`（0:02，带缩略图，`shots/stage12-real-project.png`） |
| 手机工作区 | ✅ `PHONE` 工作区在真机上生效（Program 上半屏 / Timeline·Project·Tools 下半屏） |

### 5.8 SAF save: ACTION_CREATE_DOCUMENT ✅

FilmCraft's own Save As / export dialogs now write to a real file anywhere the user points them on Android: the destination comes from `ACTION_CREATE_DOCUMENT`, through the same Java `SafBridge` + `src/saf.rs` split as the import (§5.7). **The app's own dialog code is untouched** — the host hook supplies the destination the way the desktop file dialog does.

Flow (`patches/filmcraft-android/src/lib.rs` + `src/saf.rs` + `SafBridge.java`):

1. `app.hooks.pick_save` / `pick_save_as` answer with a scratch path in the app's export directory (`Android/data/ai.storyteller.filmcraft/files/exports/<name>`) and start `spawn_publish` on a background thread;
2. the thread calls `saf.rs::pick_save` → Java `SafBridge.pickSave` (`ACTION_CREATE_DOCUMENT`, `EXTRA_TITLE` = the file name) and polls `safSaveDone()` / `safHasSaveUri()`;
3. the app writes the file to the scratch path with its ordinary file APIs; `spawn_publish` waits for the size to stop changing, then `saf.rs::publish` copies the finished file to the chosen URI.

Emulator run — Save As… from the open project → system picker (`shots/stage12-saf-save-picker.png`, name prefilled `Untitled.fcproj`), logcat:

```
I SafBridge: save destination: content://com.android.providers.downloads.documents/document/4
I main: SAF: published /storage/emulated/0/Android/data/ai.storyteller.filmcraft/files/exports/Untitled.fcproj (734 bytes) to the chosen destination
```

…and the bytes really are in the emulator's Downloads:

```bash
adb shell ls -l /sdcard/Download/
-rw-rw---- 1 u0_a204 media_rw   734 2026-10-08 14:14 Untitled.fcproj
```

Cancelling is harmless: the file just stays in the export directory (`SAF: no export destination chosen; the file stays at …`). **Outcome**: Save As / export is end-to-end on Android, with the destination entirely in the system picker.

### 5.9 Touch command palette (long-press on the menu strip) ✅

Desktop menu items are 12 pt — unreachable with a finger (§5.5). The shell now watches for a **500 ms press inside the top 8 % of the viewport** (where the desktop menu bar sits): `TouchShell::gestures` opens `command_palette`, a column of finger-sized buttons (260×52 pt, 18 pt labels) with the File commands — dispatched over the same control channel the SAF results use (§5.7):

| Button | Command sent via `engine.execute` |
|---|---|
| 打开工程… | `file.open` |
| 导入媒体… | `file.import` |
| 保存 | `file.save` |
| 另存为… | `file.saveAs` |
| 打开示例工程 | `file.openDemoProject` |
| 关闭工程 | `file.close` |

There is no new command code: each button sends `{"command": <id>, "params": {}}` and the app's *own* command runs. Two implementation details: a press is cancelled by a move of more than 16 pt, and while the finger is down the gesture asks for a repaint every 100 ms (`ctx.request_repaint_after`) — egui renders on demand, so a motionless press would otherwise never reach the 500 ms threshold.

Verified on the emulator:

```bash
adb shell input swipe 540 40 540 40 800    # 800 ms stationary press in the menu strip
```

```
I main: android: long-press on the menu strip → command palette
I main: palette: file.saveAs               # after tapping 另存为…
```

Screenshot: `shots/stage13-palette.png`.

Gotcha found while testing: the **"Recover Unsaved Changes" modal eats taps** — with it on screen the palette opens behind it and the buttons never receive the click (both are visible in the screenshot). Dismiss it (Not Now / Discard) first.

### 5.10 10-bit HEVC / 导出验证 ✅

两件事收尾：**Main10（10-bit HEVC）硬解**接进 `patches/platform-mediacodec/src/mediacodec.rs`；**真机导出**经应用自己的命令链路全链路跑通。

**10-bit HEVC 硬解**：

- 解码器接受 `bit_depth ≤ 10`（luma 与 chroma 位深必须一致），10-bit 流向设备请求 **P010** 输出（`0x36` / 54）；
- 新增 16-bit 输出路径 `frame_from_16`：把 16-bit LE 字里的样本 `>> 6` 还原成 10-bit 码值，产出 `PixelData::Yuv16 { bits: 10 }` —— 与 macOS 端 `videotoolbox.rs` 的做法完全一致；
- 仍然拒绝 4:2:2（`chroma_format_idc != 1`）、luma/chroma 位深不一致、>10 bit、隔行 —— 这些经上游 `HybridDecoder` 回退到 FilmCraft 自带的软解。

自检（`patches/filmcraft-android/src/selftest.rs`）现在解码**三段内嵌片段**（`assets/test.h264`、`test.hevc`、`test-10bit.hevc`；10-bit 片段为 7,958 字节的 HEVC Main 10、320×240、`yuv420p10le`），并且对 10-bit 片段额外用**我们自己的软解**逐帧对比 —— 守住的是 P010→10-bit 的转换，而不只是「解出了帧」。

真机（vivo PA2573 / Android 16，无线调试；2026-10-08）三段自检全部通过，logcat（15:38:34）：

```
filmcraft-android: H.264 自检: 解码器 MediaCodec H.264 | 样本 30 个 → 解出 30 帧（320x240） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0 ／ HEVC 自检: 解码器 MediaCodec HEVC | 样本 30 个 → 解出 30 帧（320x240） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0 ／ HEVC 10-bit 自检: 解码器 MediaCodec HEVC | 样本 30 个 → 解出 30 帧（320x240） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0 | 与软解对比: 30 帧与软解逐像素一致
```

同一段日志里 P010 生效的直接证据：`output_format` 含 `int32_t color-format = 54`、`stride = 640`、`slice-height = 240`，解码器 `c2.mtk.hevc.decoder`。

模拟器（emulator-5554，goldfish 没有 P010）结果同样正确（15:54:20）：硬件会话解出 0 帧，上游软件回退接住，逐像素对比通过：

```
… HEVC 10-bit 自检: 解码器 MediaCodec HEVC | 样本 30 个 → 解出 30 帧（320x240） | 硬件计数 帧 0 会话 1 拒绝 0 回退 1 | 与软解对比: 30 帧与软解逐像素一致
```

**4:2:2 的查证**（只查证、未写代码）：`dumpsys` 证据（`/tmp/dumpsys-phone.txt`、`/tmp/dumpsys-emu.txt`）—— 真机 `c2.mtk.hevc.decoder` 只广告 4:2:0 的 profile（Main、MainStill、Main10、Main10HDR10、Main10HDR10Plus）和 4:2:0 色彩格式（YUV420Flexible/Planar/SemiPlanar/PackedPlanar/PackedSemiPlanar）+ YUVP010，**没有 4:2:2（无 RExt），也没有 P210**；模拟器的 goldfish HEVC 只列 Main/MainStill，连 P010 都没有。结论：当前移动端硬件不提供 HEVC 4:2:2，解码器继续拒绝 `chroma_format_idc != 1`，这类流交给 FilmCraft 自带的软解。

**真机导出全链路**：长按菜单条 → 命令面板选「导出媒体（H.264，前 2 秒）」→ 走应用自己的引擎命令 `file.exportMedia`，显式参数 `{"format":"h264","path":"/storage/emulated/0/Android/data/ai.storyteller.filmcraft/files/exports/export-2s.mp4","range":"custom","startSeconds":0.0,"endSeconds":2.0}`。编码全程是**纯 Rust 编码器**（`crates/platform` 没有 encoder —— MTK 硬件编码明确**未接入**）。logcat（15:48）：

```
palette: 导出媒体（H.264，前 2 秒） → {…file.exportMedia…}
export: {"ok":true,"result":{"job":4,"path":"…/exports/export-2s.mp4"}}
… progress 100.0% 48/48 Done in 3.0s
export: job 4 finished: {"bytes":2975979,"frames":48,"path":"…","render_fps":15.83,"seconds":3.03}
```

同一导出跑了 3 次，输出完全一致（每次 2,975,979 字节，sha256 `7f00407754bbdb823c4bb19f46688086a846f986fe11e1771f055e11a0a7d9f5`）；`ffprobe` 复核：H.264 High 1920×1080 24000/1001 fps + AAC LC 48 kHz 立体声，容器 encoder tag `FilmCraft 0.2.1`，48/48 帧解码无错。应用静止时也能看到 `finished` 行，靠的是 `patches/filmcraft-android/src/lib.rs` 里的一处重绘修复（导出 watcher 在作业未完期间调用 `ctx.request_repaint()`，约 1 秒轮询一次）。

**如实记录两个问题**：

1. 模拟器上装完本构建**首次**启动时，10-bit 片段卡在 `c2.goldfish.hevc.decoder`（它接受了 P010 的 configure）—— 没有输出缓冲、不报错，几分钟都没出自检汇总；重启应用后不到一秒就以软件回退完成。未能复现；自检目前**没有整段解码的看门狗**。
2. 脚本化长按坐标（540 40）正好落在顶部 header 的 "Export" 模式按钮上，抬手时偶尔顺带把应用切进 Export 工作区 —— 纯外观影响：作业、日志、文件都不受影响；换一个 x 坐标即可避开。

**产物与截图**：release APK `filmcraft/apps/filmcraft-android/android/app/build/outputs/apk/release/app-release.apk` —— 44,535,267 字节，sha256 `dc70a7472a6e104b91455407bcba351dfeb64d7c4f264d8727755e3937b504cb`；截图 `shots/stage14-palette.png`、`stage14-demo.png`、`stage14-export-2s-start.png`、`stage15-palette-export.png`、`stage15-palette-export-demo.png`、`stage15-export-running.png`、`stage15-export-running-exportmode.png`、`stage15-export-finished.png`；原始日志 `/tmp/final-export-phone.log`、`/tmp/emulator-selftest.log`。

## 环境踩坑总汇（可复现）

| # | 现象 | 根因 | 解法 |
|---|---|---|---|
| 0 | Gradle/JVM 全线 `Connection refused`、依赖解析挂起 | `~/.gradle/gradle.properties` 与 `~/.npmrc` 里写死代理端口 **7890**，而实际代理（Clash Verge / verge-mihomo）在 **7897** | 已把 6 处配置统一改成 7897；构建时也可用 `-Dhttp.nonProxyHosts='*'` 直连 |
| 1 | `cargo build` 报 MSRV 不满足 / 用错 rustc | PATH 里 `/usr/local/bin`（Homebrew rust 1.89）优先于 rustup shim；且 rustup 默认工具链是 nightly 1.91 | 用 `~/.cargo/bin/cargo +stable`，并让 `PATH` 里 `~/.cargo/bin` 在前 |
| 2 | GameActivity 编译：找不到 `aarch64-linux-android-clang++` | `game-activity` 的 C++ 由 cc-rs 构建，需要 NDK 工具链环境 | 用 `cargo ndk`（它注入 CC/CXX/AR），不要裸 `cargo check` |
| 3 | Gradle wrapper 下载 `Connection refused` | `~/.gradle/gradle.properties` 写死 `systemProp.*.proxyPort=7890`，实际代理（Clash Verge / verge-mihomo）在 **7897** | 覆盖端口或 `-Dhttp.nonProxyHosts='*'` 直连；本次直接手动下载 gradle 发行版运行 |
| 4 | Gradle 依赖解析挂起（daemon 日志停在 "started executing the build"） | JVM 走本地代理时对部分仓库挂起 | `-Dhttp.nonProxyHosts='*'` 强制直连后 53 秒完成 |
| 5 | 模拟器启动后无响应、adb 看不到设备 | ① `emulator ... \| head -30` 触发 SIGPIPE 直接杀进程；② 高负载（并行 cargo 构建，load ~17）下卡在 `QEMU2 main loop`；③ 反复重启后仍卡 | 不要把长驻进程接 `head`；重活跑完再起模拟器；必要时 `-no-window -gpu swiftshader_indirect`；本机最终仍不稳定，建议真机验证 |
| 6 | GitHub 下载 | `raw.githubusercontent.com` 被重置；curl 直连 release 资产超时；`api.github.com` 匿名限额 | 用 `gh-proxy.com` / `ghproxy.net`；git 走 HTTPS/SSH-443；登录后的 `gh`（5000/h） |
| 7 | 推送到 GitHub 被拒 | 误把 `tools/`（Gradle 发行版）与 `jniLibs/*.so`（226 MB）提交 | `.gitignore` 排除 `tools/`、`**/jniLibs/`、`*.so`、`**/build/`；重置本地提交后重推 |

## 下一步

- [x] 用**真机**安装 FilmCraft APK 并验证：安装启动、硬解（H.264 / HEVC / 10-bit）、导出均已通过（2026-10-08，无线调试；见 §5.7、§5.10）
- [ ] 用真实手机重测阶段 0（手机 GPU 上限普遍 ≥8192；模拟器 4096 是已知差异）
- [x] FilmCraft 出 APK：release `app-release.apk` 已产出并验证（44,535,267 字节；见 §5.10）
- [ ] SAF 文件选择、cpal 音频、MediaCodec 硬解、触屏布局（产品级工作，见 PORTING.md）
