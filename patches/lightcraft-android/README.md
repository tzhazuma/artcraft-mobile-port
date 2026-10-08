# lightcraft-android：把 LightCraft 接到 Android

上游没有任何移动端代码，这里给出「加一个平台 crate」的最小集成路径——与 FilmCraft 试点
（`patches/filmcraft-android`，见 `HANDOFF.md`）同形，但用的是 **LightCraft 自己的缝合点与命令 id**。

## 1. 放进仓库

```bash
rsync -a patches/lightcraft-android/ <lightcraft-clone>/apps/lightcraft-android/
```

无需改根 `Cargo.toml`：workspace 成员是 `apps/*` 通配。跑 `cargo xtask ci` 时要在
`xtask/src/main.rs` 的 `LAYERS` 表里加一行 `("android", 6)`（不做只影响 xtask，不影响编译）。

## 2. 这个 crate 做了什么

上游 `apps/lightcraft` 是桌面宿主：它的 `Services`（rfd 文件对话框、`~/Pictures` 默认路径、`ui.json`
偏好）在 Android 上全都不存在。这个 crate 把那三件事换成 Android 的等价物，**引擎、UI、主题、
命令表一个字节都没改**：

| 桌面（`apps/lightcraft`） | Android（本 crate） |
|---|---|
| `std::env::args()`（`--library` / `--control` / 文件） | `AndroidApp`；库、设置、导出目录都通过 `$HOME` = 应用私有目录得到（见下） |
| `default_dir()` → `~/Pictures/LightCraft Library` | `$HOME` 被设为 `AndroidApp::internal_data_path()`，于是同一份上游代码指向 `<files>/Pictures/LightCraft Library` |
| `config_dir()` → `~/.config/lightcraft`（`ui.json`、相机配置文件、SAM 3 模型） | 同上 → `<files>/.config/lightcraft`；`PrefsWriter`（原子写：临时文件 + `fsync` + `rename`）保存语言 / 视图 / 面板等偏好 |
| `services()` 里的 rfd 对话框 | `src/saf.rs` + Java `SafBridge`：`ACTION_OPEN_DOCUMENT` 选择器 → 文件复制进 `<filesDir>/import/` → 后台线程经**应用自己的控制通道**投 `engine.execute{"command":"library.import","params":{"paths":[…]}}`（与桌面 File ▸ Add Photos 同一条命令） |
| `write` / `write_shared` / `png` | 原样调用 `lightcraft_engine::export::write_file` 与 `lightcraft_codecs::encode_png`（引擎写普通路径） |
| macOS `muda` 原生菜单栏 | 不安装（`native_menu` 保持 false），用窗口内的菜单 |
| 1600×1000 窗口 / ≥900×560 布局 | `phone_layout()`：默认进照片网格、关左栏与右栏、保留胶片条、`preview_edge ≤ 1600`；`TouchShell` 在第一帧（LightCraft 主题装好）之后把命中目标调成 40 pt，并把 `theme::Tokens` 的顶栏 / 底栏 / 工具条 / 滑块行 / 影院条加高 |

其他要点：

- **CJK 字体**：本构建没带 craft-fonts，Inter 没有汉字字形。`TouchShell::ensure_system_font` 在主题字体
  生效之后读 `/system/fonts/NotoSansCJK-Regular.ttc`（依次 NotoSerifCJK、NotoSansSC、DroidSansFallback），
  读-改-写 `ctx.fonts(|f| f.definitions().clone())`，并**追加到每一个字体族**（包括主题的
  `FontFamily::Name("semibold")`）——早于主题安装会丢掉命名族，egui 会 panic；语言切换会重建字体，
  所以每帧检查并补回。
- **触屏命令面板**：顶部 8% 长按 500 ms 打开（桌面菜单条的 12 pt 条目手指点不到），按钮直接投
  LightCraft 自己的命令 id：`file.addPhotos`、`app.openLibrary`、`dialog.export`、`view.photoGrid`、
  `view.detail`、`view.leftPanel`、`view.filmstrip`、`app.settings`、`app.about`、
  `app.language.{english,simplifiedChinese,japanese}`、`photo.saveMetadataToFile`、`app.quit`。
- **GPU 计算默认关闭**（`lightcraft_engine::gpu::set_enabled(false)` + `ui.settings.gpu = false`）：窗口本身
  仍由 eframe/wgpu 画（Vulkan），关的是 develop 流水线的第二个 wgpu 设备——手机驱动里唯一见过程序
  崩溃的地方（`gpu::backend` 的 init marker 就是为它准备的）。设置 ▸ 性能可重新打开。
- **启动自检**（`src/selftest.rs`，后台线程，从不 panic）：`Session::with_demo()` 生成示例库 →
  `lightcraft_engine::export::export_photo`（JPEG，长边 1024）渲染导出 → 用 `lightcraft_codecs::decode`
  把产物**解回来**比对尺寸 → 写进应用存储再读回。一行 logcat 同时证明 scenes、catalog、develop 流水线、
  JPEG 编解码与沙盒读写都在工作。

## 3. Gradle 外壳

`android/` 是一份可直接用的 Gradle 工程（AGP 9.1.0 / Gradle 9.3.1 / JDK 21），与 FilmCraft 试点相同，
只改了标识：

| 位置 | 值 |
|---|---|
| `app/build.gradle` → `namespace` / `applicationId` | `ai.storyteller.lightcraft` |
| `app/build.gradle` → `versionName` | `0.2.1`（= 上游 workspace 版本） |
| `AndroidManifest.xml` → `android:label` | `LightCraft` |
| `MainActivity.java` 包名 | `ai.storyteller.lightcraft` |
| `android.app.lib_name` | `main`（与本 crate `[lib] name = "main"` 一致） |
| 启动图标 | 上游自己的 `assets/app-icon/lightcraft-1024.png` 缩放成 mipmap（ArtCraft 自有素材，MIT OR Apache-2.0；不是 Adobe 素材） |

依赖 `androidx.games:games-activity:4.4.0`（**GameActivity，不是 NativeActivity**：NativeActivity 没有
IME，而且 eframe 的 `accesskit` + `native-activity` 组合是硬 compile error）。

## 4. 构建（照抄）

```bash
export PATH="$HOME/.cargo/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/28.2.13676358"
export JAVA_HOME=/opt/homebrew/opt/openjdk@21

cd ~/artcraft-mobile/lightcraft
cargo +stable ndk -t arm64-v8a -o apps/lightcraft-android/jniLibs build --release -p lightcraft-android
# → apps/lightcraft-android/jniLibs/arm64-v8a/libmain.so

cd apps/lightcraft-android/android
~/artcraft-mobile/tools/gradle-9.3.1/bin/gradle assembleRelease --no-daemon -Dhttp.nonProxyHosts='*'
# → app/build/outputs/apk/release/app-release.apk
```

（`gradlew` 用不了：`~/.gradle/gradle.properties` 里写死了代理，wrapper 下载不到发行版——见
`HANDOFF.md` §5.2。磁盘吃紧时用 `/tmp/artcraft-build-lock.sh` 串行化构建。）

安装、启动、看日志：

```bash
ADB="$ANDROID_HOME/platform-tools/adb"
APK=apps/lightcraft-android/android/app/build/outputs/apk/release/app-release.apk
"$ADB" -s emulator-5554 install -r "$APK"
"$ADB" -s emulator-5554 shell am start -n ai.storyteller.lightcraft/.MainActivity
"$ADB" -s emulator-5554 logcat | grep -Ei "lightcraft|SafBridge|panic|AndroidRuntime|wgpu"
```

成功的样子（logcat）：`android_main entered` → `HOME → /data/user/0/ai.storyteller.lightcraft/files` →
`library <…>/Pictures/LightCraft Library` → `touch mode applied` → `loaded system CJK font …` →
`library opened: N photos` → `自检: 命令 … 个 | 示例库 … 张 … | 渲染导出 … JPEG … 字节 | 回读 … | 存储 …`。

## 5. 已知缺口（首版必然缺的东西）

- **导出落点**：引擎按普通路径写文件，Android 上没有「用户可见目录」可写；导出与预设导出落在
  `<external>/Android/data/ai.storyteller.lightcraft/files/exports/`（`adb pull` / MTP 可取），
  预设导出还会弹 `ACTION_CREATE_DOCUMENT` 把成品发布到用户选的位置。照片**导出对话框**（`dialog.export`）
  的目的地来自 `pick_folder`，目前固定返回上面那个目录（SAF 目录树返回的 `content://` 树引擎按路径写不了）。
  下一步：接 `ACTION_OPEN_DOCUMENT_TREE` + 写完发布。
- **目录选择器**：同上，`pick_folder` 不弹选择器。
- **外部编辑器 / 文件管理器**：`open_with`、`reveal` 直接返回错误（Android 上没有对应动作），UI 会显示。
- **GPU 计算**：默认关闭（见 §2），未在真机上验证 SwiftShader/厂商驱动下的 compute 稳定性。
- **硬解 / 相机 profile**：LightCraft 没有 `crates/platform` 这类平台 crate，照片解码全纯 Rust，所以
  Android 不需要额外后端；相机 profile 目录随 `$HOME` 落在应用私有目录。
- **原生菜单栏**：未接（桌面 macOS 才有）；手机走长按命令面板。
- **触屏手势**：目前只有长按面板 + 40 pt 命中目标；桌面快捷键与悬停交互在手机上不可用，需要按
  FilmCraft 试点的做法逐项接（拖动 / 双指缩放 / 双击复位）。
- **真机**：模拟器已验证；真机建议用 `scripts/device-smoke.sh` 复跑（见 `HANDOFF.md`）。
