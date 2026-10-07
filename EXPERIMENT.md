# FilmCraft → Android 实验记录

> 环境：macOS 27.2/arm64，Rust stable 1.99.0，Android SDK `~/Library/Android/sdk`（AVD `Medium_Phone_API_36.0`，arm64-v8a，Android 36，Play 镜像，Chrome 133），NDK r28.2.13676358，cargo-ndk 4.1.2，JDK 27（系统）/ 21（Homebrew，备用）。

## 阶段 1：Android 工具链准备 ✅

| 项 | 状态 | 备注 |
|---|---|---|
| Rust stable ≥1.95 | ✅ 1.99.0 | 首次 `rustup toolchain install stable` 因残留状态失败，重试成功 |
| Android targets | ✅ | `aarch64-linux-android`、`x86_64-linux-android`（+ 宿主机 aarch64-apple-darwin） |
| cargo-ndk | ✅ 4.1.2 | `cargo install cargo-ndk --locked` |
| NDK r28.2.13676358 | ✅ | 原机只有 r26（16KB 页未对齐）；`sdkmanager --sdk_root=$HOME/Library/Android/sdk` 安装 |
| Java / adb / AVD | ✅ | adb 36.0.1；AVD `Medium_Phone_API_36.0` |

## 阶段 0：wasm 版在 Android 上跑通 —— ⚠️ 部分成功（发现真实阻塞点）

做法：下载官方 `filmcraft-web-0.2.1.zip`（11.5 MB）→ 本机 `python3 -m http.server 8765` → `adb reverse tcp:8765 tcp:8765` → 模拟器 Chrome 打开 `http://localhost:8765/`。

结果（有截图为证，见 `shots/`）：

1. ✅ 页面、`filmcraft_web.js`、`filmcraft_web_bg.wasm`（35 MB）全部经本地服务器加载成功（HTTP 200，见 http-server 日志）。
2. ❌ **FilmCraft 启动失败**，页面显示：

   ```
   FilmCraft failed to start:
   Limit 'max_texture_dimension_2d' value 8192 is better than allowed 4096
   ```

   即：应用向 wgpu 请求 `max_texture_dimension_2d = 8192`，而模拟器 GPU（`-gpu host` → Apple M3 Pro，经 gfxstream/OpenGL ES 3.0 转译）只允许 4096 → 设备创建被拒绝。

3. 结论与含义：
   - wasm 版在 Android 的**加载链路（静态服务/本地安全上下文/wasm 编译）没有问题**；
   - 卡点在 **GPU 能力/limits 协商**，属模拟器环境限制（模拟器 GLES 翻译层纹理上限 4096），真实手机（普遍 ≥8192）大概率能过；
   - 待验证：`?webgl` / `?cpu` 降级开关是否能绕过（模拟器重启后重测）。

## 阶段 2：filmcraft 交叉编译检查

进行中：`cargo +stable check --target aarch64-linux-android -p filmcraft-engine`（43 个 crate 的 workspace，依赖树较大）。

## 阶段 3：最小 eframe+wgpu Android 宿主（probe）

工程：`probe/egui-android-probe/`（eframe 0.36 `default-features=false` + `wgpu` + `android-game-activity`，winit 0.30，GameActivity + Gradle 9.3.1/AGP 9.1.0 骨架，拷贝自 `android-activity/examples/agdk-mainloop`）。

进行中：`cargo ndk -t arm64-v8a build` → `./gradlew assembleDebug` → 模拟器安装运行。

## 阶段 4（拉伸）：filmcraft Android 入口 crate 集成

未开始。

## 环境备注（踩坑记录）

- 模拟器第一次启动被我自己的 `emulator ... | head -30` 误杀（SIGPIPE），第二次 `nohup ... &` 正常启动；后来在并行跑两个大型 cargo 构建（load avg ~17）期间模拟器进程消失，疑似资源压力导致，需在构建结束后重启。
- 本机网络：`github.com` 直连时断（git 协议可用，curl 到 release 资产直连超时）；`raw.githubusercontent.com` 被重置；下载统一走 `gh-proxy.com` / `ghproxy.net`，git 走 HTTPS/SSH-443。
- `gh` 在 `/opt/homebrew/bin/gh`（不在 PATH 里，需显式加 PATH）。
