# FilmCraft → Android 实验记录

> 环境：macOS 27.2/arm64（M3 Pro），Rust stable 1.99.0，Android SDK `~/Library/Android/sdk`（AVD `Medium_Phone_API_36.0`，arm64-v8a，Android 36，Play 镜像，Chrome 133），NDK r28.2.13676358，cargo-ndk 4.1.2，JDK 27（系统）/ 21（Homebrew，Gradle 用它），Gradle 9.3.1 + AGP 9.1.0。

## 阶段 1：Android 工具链 ✅

| 项 | 状态 | 备注 |
|---|---|---|
| Rust stable ≥1.95 | ✅ 1.99.0 | 首次 `rustup toolchain install stable` 因残留状态失败，重试成功 |
| Android targets | ✅ | `aarch64-linux-android`、`x86_64-linux-android` |
| cargo-ndk | ✅ 4.1.2 | `cargo install cargo-ndk --locked` |
| NDK r28.2.13676358 | ✅ | 原机只有 r26（16KB 页未对齐）；用 `sdkmanager --sdk_root=$HOME/Library/Android/sdk` 安装 |
| Java / adb / AVD / Gradle | ✅ | adb 36.0.1；Gradle 9.3.1 手动下载解压（wrapper 走不通，见「踩坑」） |

## 阶段 0：wasm 版在 Android 上跑通 —— ⚠️ 加载成功，GPU 初始化被模拟器限制挡住

做法：官方 `filmcraft-web-0.2.1.zip`（10.9 MiB）→ `python3 -m http.server 8765` → `adb reverse tcp:8765 tcp:8765` → 模拟器 Chrome 打开 `http://localhost:8765/`。

结果（截图见 `shots/stage0-*.png`）：

1. ✅ 页面、`filmcraft_web.js`、`filmcraft_web_bg.wasm`（35 MB）经本地服务器全部 200 加载成功（http-server 日志为证），wasm 已开始执行。
2. ❌ FilmCraft 启动失败，页面显示：

   ```
   FilmCraft failed to start:
   Limit 'max_texture_dimension_2d' value 8192 is better than allowed 4096
   ```

   即应用向 wgpu 请求 `max_texture_dimension_2d = 8192`，而模拟器 GPU（`-gpu host` → Apple M3 Pro，经 gfxstream/OpenGL ES 3.0 转译）只给 4096 → 设备创建被拒绝。

含义：
- wasm 在 Android 上的**加载链路（本地安全上下文、静态服务、wasm 编译执行）没有问题**；
- 卡点在 **GPU limits 协商**，属模拟器环境限制（真实手机普遍 ≥8192，见「待办」）；
- 这正是「先用最便宜的方式验证可行性」要抓的那类结论：不写一行代码就能发现平台差异。

## 阶段 3：最小 eframe+wgpu Android 宿主（probe）—— 编译 ✅ / APK 打包进行中

工程 `probe/egui-android-probe/`：eframe 0.36（`default-features=false` + `wgpu` + `android-game-activity`）、winit 0.30、GameActivity + Gradle 骨架（拷自 `rust-mobile/android-activity` 的 `agdk-mainloop` 示例）。

- `cargo ndk -t arm64-v8a build` ✅ → `libmain.so`（237 MB，debug；release 会小很多）
- 关键 API 修正：**eframe 0.36 的 `App` trait 是 `fn logic(&Context)` + `fn ui(&mut Ui, &mut Frame)` 两段式**，旧教程里的 `fn update(&mut self, ctx, frame)` 已不存在（编译报 `not a member of trait`）。
- Gradle `assembleDebug` 进行中（AGP 9.1.0 + Gradle 9.3.1 + JDK 21）。

## 阶段 2 / 4：filmcraft-android 交叉编译 —— ✅ 通过（关键里程碑）

新增 `apps/filmcraft-android`（见 `patches/filmcraft-android/`）：`cdylib` + `android_main` + 复用 `Session`/`FilmcraftApp`，数据目录接 `AndroidApp::internal_data_path()`，`HostHooks::default()`（文件选择器留待 SAF 桥接）。

```bash
cargo +stable ndk -t arm64-v8a check -p filmcraft-android
# ...
#   Checking filmcraft-engine v0.2.1   Checking filmcraft-ui-egui v0.2.1
#   Checking eframe v0.36.2   Checking wgpu v30.0.1   Checking winit v0.30.13
#   Checking filmcraft-android v0.2.1
#   Finished `dev` profile [optimized + debuginfo] target(s) in 28.74s
```

**结论：FilmCraft 的完整技术栈（引擎 + egui UI + wgpu + winit + android-activity/GameActivity）可以整体交叉编译到 `aarch64-linux-android`**，且没有触发上游的 MSRV/unsafe 政策冲突（新 crate 用 `deny` + 入口 `#[allow(unsafe_code)]`，与 `crates/platform` 的既有先例一致）。下一步是出 `.so` 与 APK（见下）。

已踩到的坑（对上游移植同样成立）：

1. **`rustc` 被 PATH 抢先**：cargo 1.99 会用 PATH 上的 `rustc`；本机 PATH 里 Homebrew 的 `/usr/local/bin/rustc` 是 1.89 → 报 `rustc 1.89.0 is not supported by the following packages`。修法：`export PATH="$HOME/.cargo/bin:$PATH"`（或 `rustup default stable`）。
2. **GameActivity 的 C++ 依赖**：`winit/android-activity` 的 `game-activity` feature 会编译 GameActivity 的 C++（`game-activity-sys`，cc-rs），普通 `cargo check` 找不到 `aarch64-linux-android-clang++` → 必须用 `cargo ndk`（它负责注入 NDK 工具链环境变量），或手动设 `CC_/CXX_/AR_aarch64-linux-android`。另外注意 `cargo` 与 `cargo ndk` 都要带 `+stable`，否则会用 rustup 默认的 nightly 1.91 触发 MSRV 报错。

## 环境踩坑总汇（可复现）

| # | 现象 | 根因 | 解法 |
|---|---|---|---|
| 1 | `cargo build` 报 MSRV 不满足 / 用错 rustc | PATH 里 `/usr/local/bin`（Homebrew rust 1.89）优先于 rustup shim | 显式 `~/.cargo/bin/cargo +stable`，且 `PATH` 里 `~/.cargo/bin` 在前 |
| 2 | Gradle wrapper 下载发行版：`Connection refused` | `~/.gradle/gradle.properties` 写死 `systemProp.*.proxyPort=7890`，而实际代理（Clash Verge / verge-mihomo）在 **7897** | 用 `-Dhttps.proxyPort=7897` 覆盖；或手动下载 gradle 发行版直接运行（本次做法）；建议把该文件改成 7897 |
| 3 | 模拟器启动后无响应、adb 看不到设备 | `emulator ... \| head -30` 触发 SIGPIPE 直接杀进程；后续在 2 个大型 cargo 构建并行（load ~17）时卡死 | 不要把长驻进程接 `head`；重活跑完再起模拟器；`-no-boot-anim` 缩短启动 |
| 4 | GitHub 下载 | `raw.githubusercontent.com` 被重置；curl 直连 release 资产超时；`api.github.com` 匿名限额 | 下载走 `gh-proxy.com` / `ghproxy.net`；git 走 HTTPS/SSH-443；用登录后的 `gh`（5000/h） |
| 5 | cargo 下载卡住 | 偶发（与并行构建/代理状态有关） | `CARGO_HTTP_MULTIPLEXING=false` + 重试 |

## 待办 / 下一步

- [ ] probe APK 装到模拟器跑起来（截图）→ 证明 eframe 0.36 + wgpu + GameActivity 链路可用
- [ ] 用真实手机重测阶段 0（手机 GPU 上限普遍 ≥8192；模拟器 4096 是已知差异）
- [ ] `filmcraft-android` 编译通过后出 APK（需 Gradle 外壳，见 patches 里的 README）
- [ ] SAF 文件选择、cpal 音频、MediaCodec 硬解、触屏布局（产品级工作，见 PORTING.md）
