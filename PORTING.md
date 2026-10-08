# ArtCraft 系列（PhotoCraft/FilmCraft/LightCraft/EffectCraft/PrintCraft）移动端移植方案

> 目标平台：Android、iOS、鸿蒙（HarmonyOS NEXT 移动端 + HarmonyOS PC）
> 依据：对 `storytold/*` 仓库源码、CI、发布产物与相关生态（egui/wgpu/winit、ohos-rs、Tauri、Capacitor）的逐项核查；所有「未确认」项均已标注。

---

## 1. 项目与技术栈

| 项 | 事实 |
|---|---|
| 项目 | GitHub 组织 `storytold`（ArtCraft / Crafting Apps），纯 Rust「洁净室重写」Adobe 系列：photocraft(PS)、vectorcraft(AI)、filmcraft(PR)、lightcraft(LR)、effectcraft(AE)、printcraft(Acrobat)、designcraft(InDesign) 等 |
| 许可 | `MIT OR Apache-2.0`（GitHub 页面显示 Apache-2.0） |
| GUI | **egui / eframe 0.36**（immediate-mode） |
| 渲染 | **wgpu 30**（WGSL 合成器；Web 端 WebGPU → WebGL2 回退） |
| 窗口 | **winit 0.30**（eframe 依赖 `^0.30.13`） |
| 媒体 | 纯 Rust 编解码（H.264/HEVC/VP9/AV1/AAC… + MP4/MKV/MXF 容器），**无 ffmpeg / 无 C 依赖** |
| 现有平台 | macOS（universal DMG，已签名+公证）、Windows、Linux、FreeBSD、**WASM（每应用都有 `apps/<app>-web`）** |
| 移动端现状 | **零代码、零路线图**（全树无 android/ios/gradle/jni/xcode/ohos）→ 移植从零开始 |
| MSRV | photocraft/filmcraft/effectcraft = 1.95；lightcraft/printcraft = 1.90 |
| 工程约束 | workspace `unsafe_code = "forbid"`；唯一例外 `crates/platform`（macOS VideoToolbox FFI，受 ADR 0001 约束）——**任何新平台 FFI 都要新建独立 crate 并修订 ADR** |

**移植的着力点（上游已经准备好的抽象缝）**：

| 缝 | 位置 | 桌面实现 | 需要新平台提供的 |
|---|---|---|---|
| `Services`（文件系统） | `crates/engine` | `FsServices`（std::fs） | Android/iOS 沙盒路径或 SAF/Document Picker |
| `HostHooks`（宿主钩子） | `crates/ui-egui/src/lib.rs` | rfd 文件对话框 | 系统文件选择器 |
| `AudioOut/AudioInput` | `ui-egui` / `engine` | cpal | cpal 自带 Android/iOS 后端（需权限配置） |
| `filmcraft_platform::register()` | `crates/platform` | macOS VideoToolbox | MediaCodec / VideoToolbox(iOS) / OHOS AVCodec（可先不做，退软解） |

---

## 2. 移植总览：三条路线

| 路线 | 做法 | 适用 | 成本 | 主要风险 |
|---|---|---|---|---|
| **A 原生移植** | 同一套 egui/wgpu 代码编到目标平台 | 想要真正的原生 App | 高 | iOS 27 强制 UIScene；鸿蒙 winit 版本断层；wgpu 在 OHOS 只能 GLES |
| **B WebView 包装** | 复用现成 wasm 版，套 Tauri v2 / Capacitor（鸿蒙用 ArkWeb） | 快速验证 / 快速上线 | 低 | 安全上下文（WebGPU/OPFS 依赖）；触屏 UI 未适配；性能上限=系统 WebView |
| **C 原生核心 + 系统外壳** | Rust 核心库（uniffi/FFI）+ SwiftUI / ArkTS 重写 UI | 想要移动原生体验、上架 | 中高 | 等于重写 UI（egui 面板/时间线无法 1:1 复刻） |

**推荐组合**：先用 B 做可行性验证与快速出成果 → Android 走 A（生态最成熟）→ iOS 先 spike「UIScene + wgpu + 中文输入」再定 A/B → 鸿蒙先做 NAPI 最小实验，正式路径 PC 优先 ArkWeb、移动端评估 winit-ohos。

---

## 3. 通用准备

```bash
# Rust ≥1.95（本实验机：stable 1.99.0）
rustup toolchain install stable
# 注意：不要急着改全局 default（可能影响其它项目），用 `cargo +stable ...` 显式指定

# 各平台 target
rustup target add --toolchain stable \
  aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android \
  aarch64-apple-ios aarch64-apple-ios-sim \
  aarch64-unknown-linux-ohos x86_64-unknown-linux-ohos \
  wasm32-unknown-unknown
```

新平台的标准做法（与上游 `apps/filmcraft-web` 的先例一致）：**新建一个 L6 app crate**（`apps/<app>-<platform>`），复用 engine + ui-egui，只实现上表的三个缝；不要动 `crates/ui-egui` 的内部结构。

---

## 4. Android（路线 A，生态最成熟）

### 4.1 工具链

```bash
rustup target add --toolchain stable aarch64-linux-android x86_64-linux-android
cargo +stable install cargo-ndk --locked
sdkmanager --sdk_root="$HOME/Library/Android/sdk" --install "ndk;28.2.13676358"   # 必须 r28+（16KB 页对齐）
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/28.2.13676358"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
```

### 4.2 入口 crate

```rust
// apps/<app>-android/src/lib.rs
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]                     // edition 2024：必须 unsafe(...)；该 crate lint 需从 forbid 降为 deny
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let options = eframe::NativeOptions {
        android_app: Some(app),          // 缺这个字段 eframe 会直接报错
        ..Default::default()
    };
    eframe::run_native("FilmCraft", options, Box::new(|cc| Ok(Box::new(FilmcraftApp::new(cc))))).unwrap();
}
```

要点：
- `[lib] name = "main", crate-type = ["cdylib"]`（与 `android.app.lib_name` 一致）。
- eframe features 按目标拆分：Android 用 `["default_fonts","wgpu","accesskit","android-game-activity"]`。
  - **accesskit + native-activity 是 eframe 硬 `compile_error!`**；
  - NativeActivity 无软键盘/IME，面板+文本输入型应用**必须 GameActivity**。
- 不要直接依赖 `android-activity`（用 winit 再导出的 `winit::platform::android::activity::*`）。

### 4.3 Gradle 骨架

```
app/src/main/AndroidManifest.xml    # activity + configChanges + <meta-data android:name="android.app.lib_name" android:value="main"/>
app/src/main/java/.../MainActivity.java   # extends GameActivity; static { System.loadLibrary("main"); }
app/build.gradle                    # minSdk 31 / compileSdk 35 / Java 17 / androidx.games:games-activity:4.4.0（勿开 prefab）
app/src/main/jniLibs/arm64-v8a/libmain.so # cargo ndk 产物
```

```bash
cargo ndk -t arm64-v8a -o app/src/main/jniLibs build --release
./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n <pkg>/.MainActivity
adb logcat | grep -Ei "rust|panic|wgpu|vulkan|AndroidRuntime"
```

快速起骨架可用 `cargo-mobile2`（含 egui+winit+wgpu 模板）；长期建议自维护上述骨架。

### 4.4 渲染与设备

- wgpu 30 默认编译 `vulkan` + `gles`；真机走 Vulkan，GLES 3.0 兜底（可用 `WGPU_BACKEND=gl` 强制）。
- **Apple Silicon 模拟器**：Vulkan 经 MoltenVK 转译，特性不全 → 失败时 `emulator -gpu swiftshader` 或 `-feature -Vulkan`。
- **16 KB 页政策**（硬约束）：NDK r28+ 默认对齐；旧 NDK 需 `-Wl,-z,max-page-size=16384`；验证：
  `llvm-readelf -lW libmain.so | grep LOAD`（期望 `0x4000`）、`zipalign -c -P 16 -v 4 app.apk`。
- minSdk：cpal 的 Android 后端要求 **≥26**；官方 GameActivity 示例用 31。

### 4.5 FilmCraft 专属改造清单（按优先级）

| # | 问题 | 处理 |
|---|---|---|
| 1 | `rfd` 无 Android 后端（**编译期硬阻塞**） | rfd 限定桌面；Android 实现 `HostHooks`（SAF `ACTION_OPEN_DOCUMENT`；或「私有目录 + 进出拷贝」起步） |
| 2 | `default_data_dir()` 在 Android 返回 None | 接 `AndroidApp::internal_data_path()`（自动保存/崩溃恢复） |
| 3 | 剪贴板被 egui-winit 编译掉 | JNI `ClipboardManager`（可选） |
| 4 | `open_path()` 用 `Command::new("open")` | 换 Intent |
| 5 | 字体目录不含 Android | 加 `/system/fonts` |
| 6 | `FILMCRAFT_CPU_COMPOSITE` 是环境变量 | 改设置开关 |
| 7 | 无硬解（只有 VideoToolbox） | 新写 MediaCodec 后端（新 FFI + 修订 ADR 0001） |
| 8 | UI 为桌面设计（1600×980、143 快捷键、悬停） | 触屏布局属产品级工作量，不在实验范围 |

### 4.6 实测补充（2026-10-08，本仓库实验）

| 主题 | 结论 |
|---|---|
| **release 体积** | `cargo ndk ... build --release`：FilmCraft `.so` 855 MB（debug）→ **45 MB**；APK 42 MB（含 debug 签名）。debug 产物不适合装机 |
| **字体** | egui 默认字体无 CJK → 中文渲染成方块。运行时读 `/system/fonts/NotoSansCJK-Regular.ttc` 挂到 `FontDefinitions.families` 尾部即可（`.ttc` 用 `FontData.index` 选面）；上游等效方案是随包带 craft-fonts |
| **硬解（MediaCodec）** | 用 `ndk` crate 的 AMediaCodec 绑定（`features=["media","api-level-31"]`）已跑通：`c2.goldfish.h264.decoder`，输出 320×240。**要接进上游 `crates/platform`**（该 crate 是唯一允许 `unsafe` 的 FFI 白名单），并注意按访问单元(AU)逐帧送输入，不要整段塞一个 buffer |
| **触屏 UI** | 桌面布局在手机上不可用（菜单重叠、命中目标几像素）。实测可行的组合：底部大按钮工具条（≥48dp）+ `dragged()` 平移 + `zoom_delta()`/`multi_touch()` 双指缩放 + `double_clicked()` 复位 + `long_touched()` 菜单 |
| **UI 在真机/模拟器都能起** | wgpu 在模拟器选 Vulkan(SwiftShader) 可稳定运行；早期一次探针 Vulkan 崩溃定位在模拟器驱动 `vulkan.ranchu.so`，可用 `WGPU_BACKEND=gl` 规避 |
| **16 KB 页** | NDK r28 + AGP 9.1 的产物：`zipalign -c -P 16` 通过、`.so` LOAD 段 `0x4000` 对齐 ✅ |

---

## 5. iOS

### 5.1 路线 A：eframe 直跑（复用 100% UI）

```bash
rustup target add --toolchain stable aarch64-apple-ios aarch64-apple-ios-sim
cargo +stable check --target aarch64-apple-ios -p filmcraft-ui-egui --no-default-features   # iOS 必须关 glow/glutin/wayland/x11/accesskit
```

- eframe 在 iOS 上要 `default-features = false, features = ["default_fonts","wgpu"]`（glutin 编不过 iOS，egui 官方 CI 即如此）。
- 新增 `apps/filmcraft-ios`（`crate-type=["staticlib"]`，导出 `filmcraft_start_app()`），Xcode 工程用 XcodeGen：
  `OTHER_LDFLAGS=[-lc++,-ObjC]`、`ENABLE_USER_SCRIPT_SANDBOXING=NO`、`ARCHS=arm64`、postCompile 调 cargo。

**头号阻断（先 spike，失败就别走 A）**：iOS 27 SDK 要求 `UIApplicationSceneManifest`，缺失**启动即失败**（Apple TN3187）；winit 尚未支持 UIScene（issue #4224 未关）→ 需自写 `SceneDelegate` 把 winit 窗口挂到 `UIWindowScene`（参考 MIT 的 bevy_ios_toolkit；**不可抄 Slint，GPL**）。

**已知缺口**：winit iOS 不支持 `Ime` 事件 → **中文输入法不可用**；手势需显式开启（`recognize_pinch_gesture` 等）；`rfd` 无 iOS 后端 → `UIDocumentPickerViewController`；后台导出会被挂起 → `beginBackgroundTask`。

### 5.2 路线 B：Rust 核心 + SwiftUI（uniffi）

```bash
cargo +stable build --release --target aarch64-apple-ios -p <core-crate>
cargo run -p uniffi-bindgen -- generate --library target/aarch64-apple-ios/release/lib<core>.a --language swift --out-dir ios/generated
xcodebuild -create-xcframework -library ... -output <Core>.xcframework
```

SwiftUI `App` + `WindowGroup` 天然满足 UIScene 要求；但 UI 要重写——适合做「选片→预览→粗剪→导出」的精简版。

### 5.3 路线 C：WKWebView + 现成 wasm（最快验证）

SwiftUI/UIKit + `WKWebView` 加载 `filmcraft-web` 产物；先实测 `window.isSecureContext` 与 `navigator.gpu`（第三方 WKWebView 的 WebGPU 支持**未确认**）。

### 5.4 Info.plist / 上架要点

- 必需：`UIApplicationSceneManifest`、`UISceneDelegateClassName`、`UILaunchScreen`、`CADisableMinimumFrameDurationOnPhone`、`PrivacyInfo.xcprivacy`（强制）、`UIRequiredDeviceCapabilities=["arm64"]`、`UIFileSharingEnabled`、`LSSupportsOpeningDocumentsInPlace`、`ITSAppUsesNonExemptEncryption`、麦克风/相册用途描述。
- 上架：2026-04-28 起须 Xcode 26+/iOS 26 SDK；最低部署目标 ≥iOS 13；元数据**不得使用 Adobe 商标**（Guideline 5.2.1/4.1）；wasm 走 Guideline 2.5.2 需解释（打包在 App 内、不下载）。

---

## 6. 鸿蒙（HarmonyOS NEXT 移动端 + HarmonyOS PC）

### 6.1 现状

| 维度 | 移动端 | PC（鸿蒙电脑） |
|---|---|---|
| deviceTypes | `phone` / `tablet` | **`2in1`**（官方取值；`pc` 不是） |
| 芯片 | ARM64 | ARM64（无 x86_64 正式版） |
| 应用 | HAP/APP，同一套 ArkTS/ArkUI | 同一套 HAP；PC 需窗口/键鼠/文件选择器适配 |
| 工具 | DevEco Studio（macOS ARM 支持；磁盘要求 100GB+） | 同一 IDE |

Rust 侧：`aarch64-unknown-linux-ohos` 已是 **Tier 2 with Host Tools**（`rustup target add` 直接装 std）。**陷阱：`target_env = "ohos"`、`target_os = "linux"`，判定必须用 `#[cfg(target_env = "ohos")]`。**

### 6.2 路线 1：NAPI（ArkTS 调 Rust，最稳）

```bash
export DEVECO_SDK_HOME="/Applications/DevEco-Studio.app/Contents/sdk"
export OHOS_NDK_HOME="$DEVECO_SDK_HOME/default/openharmony"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER="$OHOS_NDK_HOME/native/llvm/bin/aarch64-unknown-linux-ohos-clang"
cargo install ohrs                                  # ohos-rs 构建 CLI
cargo new --lib filmcraft-ohos && cd filmcraft-ohos
cargo add napi-ohos napi-derive-ohos && cargo add napi-build-ohos --build
ohrs build --release -a aarch                        # → lib<name>.so + index.d.ts
```

DevEco 工程：`.so` 放 `entry/libs/arm64-v8a/`，`.d.ts` 放 `entry/src/main/cpp/types/libxxx/`，ArkTS 里 `import native from 'libxxx.so'`；`hvigorw assembleHap` → `hdc install`。

### 6.3 路线 2：原生 GUI（Rust 自己管窗口）

- 窗口/事件：`winit-ohos`（winit-core 0.31-beta）或 `openharmony-ability` + `cargo-ohos-app`（可产出 `.hap`）。
- **与 eframe 的冲突**：eframe 0.36 依赖 winit `^0.30.13`，而 winit-ohos 跟 0.31-beta → 需换 `richerfu/winit` master，或改用 gpui-ohos 路径（参考实现）。
- 渲染：`XComponent(SURFACE)` → `OH_NativeWindow_CreateNativeWindowFromSurfaceId` → EGL(`EGL_DEFAULT_DISPLAY`) → **wgpu 只开 `gles` feature 并 pin git rev**（Vulkan 被 ash 未发版阻塞；GLES 有 srgb 已知 issue）。
- 硬解：另写 OHOS AVCodec 后端（`OH_AVCodec_GetCapability` → `CreateByName` → 失败回退 `CreateByMime`）。

### 6.4 路线 3：ArkWeb + 现成 wasm（最快，PC 最佳落点）

wasm 产物拷进 `entry/src/main/resources/rawfile/web/`，用 `onInterceptRequest` 把 `https://<app>.local/*` 映射到 rawfile 并**显式设 MIME**（`.wasm → application/wasm`）：

```ts
Web({ src: 'https://filmcraft.local/index.html', controller: this.controller })
  .onInterceptRequest((event) => {
    const url = event?.request.getRequestUrl() ?? '';
    if (!url.startsWith('https://filmcraft.local/')) return null;
    const name = url.replace('https://filmcraft.local/', '').split('?')[0];
    const mime = name.endsWith('.wasm') ? 'application/wasm'
               : name.endsWith('.js') ? 'text/javascript' : 'text/html';
    const r = new WebResourceResponse();
    r.setResponseData($rawfile(`web/${name}`));
    r.setResponseMimeType(mime); r.setResponseCode(200); r.setResponseIsReady(true);
    return r;
  })
```

坑：`resource://` 不支持 `fetch` 且不是安全上下文；ArkWeb 的 WebGPU/WebCodecs/OPFS 支持情况**未确认**（Chromium M132，理论支持）→ 用 `?webgl` 保底（WebGL2 = CPU 合成，性能有限）。

### 6.5 门槛

编译不需要账号；**真机安装/调试需要华为账号 + 实名认证 + 调试签名**；上架走 AGC。工程 `deviceTypes: ["phone","tablet","2in1"]` 一步到位。

---

## 7. 快速包装路线（Tauri v2 / Capacitor）

| | Capacitor 8.5.3 | Tauri v2 2.12.1 |
|---|---|---|
| 平台 | iOS/Android/Web（**无鸿蒙**） | iOS/Android/桌面 |
| 资源加载 | Android `https://localhost`（安全上下文 ✓）/ iOS `capacitor://localhost`（✗ 存疑） | Android 可 `useHttpsScheme=true` / iOS `tauri://localhost`（✗ 存疑） |
| 命令 | `npx cap init <App> <bundle> --web-dir=www && npx cap add android && npx cap run android` | `npm create tauri-app@latest` → `npx tauri android init && npx tauri android dev` |

共同风险：WebView 内 WebGPU/WebCodecs/OPFS 可用性需实测；导出/保存需原生下载监听；触屏 UI 未适配。

---

## 8. 工作量与优先级

| 目标 | 推荐路线 | 相对工作量 | 前置条件 |
|---|---|---|---|
| Android 原生 | A | 中 | 工具链 + NDK r28+；触屏 UI 另算 |
| iOS | C（精简版）或先 WKWebView 验证 | 高（UIScene/IME） | Xcode + 开发者账号（真机/上架） |
| 鸿蒙移动 | NAPI 实验 → ArkWeb 或 winit-ohos | 高 | DevEco + 真机 + 华为账号实名 |
| 鸿蒙 PC | ArkWeb | 中 | 同上 |

---

## 9. 未确认 / 风险清单

1. **winit 的 iOS UIScene 支持**：issue #4224 未解决；必须实测（Xcode 27 编译最小 winit app）。
2. **WKWebView 的 WebGPU / WebCodecs / OPFS**（第三方 App 内）：未确认，必须真机实测。
3. **ArkWeb 的 WebGPU / WebAssembly / WebCodecs**：无官方明确条目，理论支持（Chromium 132/144），必须真机实测。
4. **eframe + wgpu 在 Android 的稳定性**：egui 官方示例用 glow + native-activity，**没有 wgpu-on-Android 官方示例**；GameActivity + wgpu 可能需要额外适配。
5. **`EGL_PLATFORM_OHOS_KHR`** 是否存在：未确认（官方 sample 用 `EGL_DEFAULT_DISPLAY`）。
6. **wgpu 的 OHOS 支持**：需要 pin git rev + `gles` feature（社区做法）；发布版可能需要补丁。
7. **许可污染**：Slint 的 iOS 场景委托是 GPL/royalty-free，与上游 clean-room 政策冲突，只能用 MIT 参考（bevy_ios_toolkit）。
8. **上游高速迭代**：5 个仓库近 24h 均有推送，无移动端 roadmap；移植方案需按「跟上游同步」设计，避免 fork 漂移。

## 10. 参考链接

- 上游：<https://github.com/storytold>（各应用仓库与 `craft-fonts`、`photocraft-corpus`）
- egui/eframe：<https://github.com/emilk/egui>（iOS CI 步骤见其 `rust.yml`）
- winit：<https://github.com/rust-windowing/winit>（iOS #4224 / OpenHarmony #4081）
- android-activity：<https://github.com/rust-mobile/android-activity>（GameActivity 示例 `examples/agdk-mainloop`）
- cargo-ndk：<https://github.com/bbqsrc/cargo-ndk>
- cargo-mobile2：<https://github.com/tauri-apps/cargo-mobile2>
- ohos-rs：<https://ohos.rs/>；cargo-ohos：<https://github.com/openharmony-rs/cargo-ohos>；winit-ohos：<https://github.com/ohos-rs/winit-ohos>
- 鸿蒙 XComponent：<https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/ui/napi-xcomponent-guidelines.md>
- Tauri v2：<https://v2.tauri.app/>；Capacitor：<https://capacitorjs.com/>
