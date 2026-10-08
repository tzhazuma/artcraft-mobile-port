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

**真正的触屏 UI 不是缩放问题，是布局问题**（下一步工作）：底部大按钮工具条替代顶部菜单；一次只显示一个面板；时间线占满宽度；把 143 个快捷键换成手势 + 长按菜单。探针里已验证过这些交互（§5.2）。

顺带验证：**系统 CJK 字体加载生效**（logcat: `loaded system CJK font /system/fonts/NotoSansCJK-Regular.ttc`）。

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

- [ ] 用**真机**（USB 调试）安装 `app-debug.apk` 与 FilmCraft APK：`adb install -r ...`，观测 wgpu 后端（Vulkan）与 limits 是否满足 8192
- [ ] 用真实手机重测阶段 0（手机 GPU 上限普遍 ≥8192；模拟器 4096 是已知差异）
- [ ] FilmCraft 出 APK：按 `patches/filmcraft-android/README.md` 复制 Gradle 外壳；建议先做 `--release`（debug 的 855 MB .so 不实用）
- [ ] SAF 文件选择、cpal 音频、MediaCodec 硬解、触屏布局（产品级工作，见 PORTING.md）
