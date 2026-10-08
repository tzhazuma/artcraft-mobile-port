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
