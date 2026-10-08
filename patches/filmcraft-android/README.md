# filmcraft-android：把 FilmCraft 接到 Android

上游没有任何移动端代码，这里给出「加一个平台 crate」的最小集成路径（与上游 `apps/filmcraft-web` 的做法一一对应）。

## 1. 放进仓库

```bash
cp -R patches/filmcraft-android <filmcraft-clone>/apps/filmcraft-android
```

无需改根 `Cargo.toml`：workspace 成员是 `apps/*` 通配。

## 2. 注册分层（跑 `cargo xtask ci` 时需要）

在 `xtask/src/main.rs` 的 `LAYERS` 表里加一行：

```rust
("android", 6),
```

（`short()` 会把 `filmcraft-android` 归为 `android`。不做这一步只影响 `cargo xtask ci`，不影响编译。）

## 3. Gradle 外壳

把本仓库 `probe/egui-android-probe/` 的 Gradle 骨架（`gradlew`、`gradle/`、`build.gradle`、`settings.gradle`、`gradle.properties`、`app/`）复制到 `apps/filmcraft-android/android/`，然后把这些值改成 FilmCraft：

| 位置 | 改成 |
|---|---|
| `app/build.gradle` → `namespace` / `applicationId` | `ai.storyteller.filmcraft` |
| `app/src/main/AndroidManifest.xml` → `android:label` | `FilmCraft` |
| `app/src/main/java/.../MainActivity.java` 的包名 | 与 namespace 一致 |
| （不用改）`android.app.lib_name` | `main` — 与本 crate 的 `[lib] name = "main"` 一致 |

## 4. 构建

```bash
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/28.2.13676358"
cd <filmcraft-clone>
cargo ndk -t arm64-v8a -o apps/filmcraft-android/android/app/src/main/jniLibs build -p filmcraft-android
cd apps/filmcraft-android/android && JAVA_HOME=/opt/homebrew/opt/openjdk@21 ./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n ai.storyteller.filmcraft/.MainActivity
adb logcat | grep -Ei "filmcraft|rust|wgpu|vulkan|AndroidRuntime"
```

## 5. 已知缺口（首版必然缺的东西）

- **文件选择/导入导出**：`rfd` 没有 Android 后端，Android 侧要写自己的 `HostHooks`（SAF：`ACTION_OPEN_DOCUMENT` / `ACTION_OPEN_DOCUMENT_TREE`），或先只支持应用私有目录。
- **持续化与恢复**：已接 `AndroidApp::internal_data_path()`（自动保存、崩溃日志、偏好）。
- **音频**：cpal 有 Android 后端（AAudio，minSdk ≥ 26），但还要处理 `RECORD_AUDIO` 权限与 `AudioManager` 路由。
- **硬解**：只有 macOS VideoToolbox；Android 需新写 MediaCodec 后端（上游 `crates/platform` 是唯一允许 `unsafe` 的 crate，按 AGENTS.md §0.3 要在那里加）。
- **触屏 UI**：egui 版界面是按桌面设计的（1600×980、143 个快捷键、悬停菜单），手机上需要单独做触控布局。
- **`unsafe_code` 政策**：本 crate 用 `deny` + 入口处 `#[allow(unsafe_code)]`（`#[unsafe(no_mangle)]` 需要），与 `crates/platform` 的既有先例一致；上游若合并，建议同步更新 ADR 0001。

## 6. Gradle 外壳（本仓库已附）

`patches/filmcraft-android/android/` 是一份可直接用的 Gradle 工程（AGP 9.1.0 / Gradle 9.3.1 / JDK 21）：

```bash
cargo +stable ndk -t arm64-v8a -o apps/filmcraft-android/jniLibs build --release -p filmcraft-android
cd apps/filmcraft-android/android && ./gradlew assembleRelease -Dhttp.nonProxyHosts='*'
adb install -r app/build/outputs/apk/release/app-release.apk
adb shell am start -n ai.storyteller.filmcraft/.MainActivity
```

release 产物实测：`.so` 45 MB、APK 42 MB（debug 分别是 855 MB / 不出包）；`jniLibs` 通过
`sourceSets { main { jniLibs.srcDirs = ['../../jniLibs'] } }` 指向 cargo-ndk 的输出目录。

## 7. 硬解（MediaCodec）接入方案 —— 已验证可行性，待实现

探针（`probe/egui-android-probe/src/mediacodec.rs`）已证明 AMediaCodec 路径可用
（`c2.goldfish.h264.decoder`，输出 320×240；按访问单元逐帧送输入时可连续出帧）。

接入上游 `crates/platform` 需要的（接口已核对）：

1. **工厂**：`register()` 里对 `target_os = "android"` 注册一个 `videotoolbox_factory` 同形的
   工厂 —— 入参 `&filmcraft_isobmff::SampleEntry`，出参 `Option<Result<Box<dyn filmcraft_codecs::VideoDecoder>>>`；
   能从 `avcC`/`hvcC` 里拿到 SPS/PPS（作为 MediaCodec 的 `csd-0`/`csd-1`）。
2. **解码器实现 `VideoDecoder`**：`decode(&mut self, sample: &[u8], pts: i64) -> Result<Vec<DecodedFrame>>`
   —— 注意上游的 `sample` 是 **avcC 长度前缀**格式，喂 MediaCodec 前要转成 Annex-B
   （把 4 字节长度换成 `00 00 00 01` 起始码）。
3. **帧转换**（工作量主体）：MediaCodec 输出 `COLOR_FormatYUV420Flexible`（常见 NV12），
   要按输出格式里的 **stride / slice-height / crop** 裁切并构造
   `filmcraft_frame::VideoFrame { width, height, data: PixelData, color: ColorInfo, par, pts }`；
   参考实现是 `crates/platform/src/videotoolbox.rs::copy_out`。
4. **回退语义**：上游要求硬件工厂「不支持的流要拒绝」，中途失败要 `note_hw_fallback()` 并切回
   `software_video_decoder`；统计计数走 `filmcraft_codecs::hw::{note_hw_session,note_hw_frames,note_hw_declined,note_hw_fallback}`。
5. **零 unsafe**：`ndk` crate 已包好 FFI，本后端可以用安全 Rust 写；`crates/platform` 的
   `unsafe_code = "deny"` 不需要额外豁免。

## 8. 硬解已接入（2026-10-08）

- `crates/platform` 的 MediaCodec 后端见 `patches/platform-mediacodec/`（覆盖该 crate 的三个文件）；
- 本 crate 的 `android_main` 现在会调用 `filmcraft_platform::register()` 并打印
  `hardware decoding: Available("MediaCodec")`；
- 启动后另起线程跑一次自检（`src/selftest.rs`，内嵌 `assets/test.h264`），logcat 输出例如：

  ```
  自检: 解码器 MediaCodec H.264 | 样本 30 个 → 解出 30 帧（319x239） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0
  ```

  （自检走的是引擎自己的 `make_video_decoder()` 路径，所以它同时证明了工厂注册、HybridDecoder
  包装与 hw 计数都在工作。）

## 9. 文件导入（File ▸ Import ▸ 系统选择器）

`HostHooks` 是同步回调，不能在里面等用户选文件，所以：

1. `app.hooks.pick_files` / `pick_open_project` / `pick_open_file` 里**异步**启动 SAF 选择器（`src/saf.rs`
   调 Java `MainActivity.safPickMedia()`），当帧返回「没有文件」；
2. 用户选完后 `SafBridge` 把文件复制进 `<filesDir>/import/` 并把路径写进 `<filesDir>/import-manifest.txt`；
3. 后台线程读到清单后，经控制通道投
   `engine.execute{"command":"file.import","params":{"paths":[…]}}`（`menus::invoke` 带 paths 时直接执行
   引擎命令，就是桌面 Import 的同一条命令）；
4. `docs/control-protocol.md` 里的 `engine.execute` 也是给自动化/MCP 的同一入口。

实测（模拟器）：`file.import replied {"ok":true,"result":{"errors":[],"items":[1]}}`，Project 面板出现素材。
注意：只接了「导入 / 打开」，导出用的 `pick_save` 等仍是空实现（SAF 的 `ACTION_CREATE_DOCUMENT` 是下一步）。
