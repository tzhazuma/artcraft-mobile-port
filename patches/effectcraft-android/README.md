# effectcraft-android：把 EffectCraft 接到 Android

上游没有任何移动端代码，这里给出「加一个平台 crate」的最小集成路径——与 FilmCraft 试点
（`patches/filmcraft-android`，见 `HANDOFF.md`）同形，但用的是 **EffectCraft 自己的缝合点与命令 id**。

## 1. 放进仓库

```bash
rsync -a patches/effectcraft-android/ <effectcraft-clone>/apps/effectcraft-android/
```

无需改根 `Cargo.toml`：workspace 成员是 `apps/*` 通配。跑 `cargo xtask ci` 时要在
`xtask/src/main.rs` 的 `LAYERS` 表里加一行 `("android", 6)`（不做只影响 xtask，不影响编译）。

## 2. 这个 crate 做了什么

上游 `apps/effectcraft` 是桌面宿主：它的 `Hooks`（rfd 文件对话框）、cpal 音频输出、
`effectcraft_host::config_dir()`（`~/.config/effectcraft`）在 Android 上都不存在。这个 crate 把它们
换成 Android 的等价物，**引擎、UI、主题、命令表一个字节都没改**：

| 桌面（`apps/effectcraft`） | Android（本 crate） |
|---|---|
| `std::env::args()`（`--control` / `--demo` / 工程与素材文件） | `AndroidApp`；配置目录、自动保存、字体目录都通过 `$HOME` = 应用私有目录得到 |
| `effectcraft_host::config_dir()` → `~/.config/effectcraft` | `$HOME` 被设为 `AndroidApp::internal_data_path()`，于是同一份上游代码指向 `<files>/.config/effectcraft`（`DirConfig` 写设置、自动保存与插件） |
| `Hooks` 里的 rfd 对话框 | `src/saf.rs` + Java `SafBridge`：`ACTION_OPEN_DOCUMENT` 选择器 → 文件复制进 `<filesDir>/import/` → 后台线程经**应用自己的控制通道**投 `engine.execute{"command":"file.import","params":{"paths":[…]}}`（`file.open` 用 `{"path":…}`）——与桌面 File ▸ Import / Open Project 同一条命令 |
| `pick_save` / `pick_save_file`（工程另存为、渲染帧另存） | 引擎按普通路径写文件，所以 hook 先返回 `<external>/…/exports/` 里的临时路径，后台线程弹 `ACTION_CREATE_DOCUMENT`，等文件写完（大小稳定 ≥1.5 s）后把成品拷到用户选的位置；用户取消则文件留在 exports 目录，不丢 |
| `pick_folder`（设置路径、Collect Files、Watch Folder） | 固定返回应用的 exports 目录（SAF 目录树给的 `content://` 树引擎按路径写不了） |
| cpal 音频输出（`audio_out.rs`） | **未接**（`audio_device: None`）：预览静音播放，不报错；接 AAudio 还需要权限与路由（PORTING.md §1） |
| 1680×1020 窗口 / ≥960×600 布局 | `phone_layout()` 通过应用自己的 dock 模型装一个 **"Phone" 工作区**（Composition 在上、Timeline 在下，Project / Effect Controls 作为同一 dock 的标签页），并进 `saved_workspaces`，所以 Window ▸ Workspace 里能选、用户能换走；`TouchShell` 在第一帧（EffectCraft 主题装好）之后把命中目标调成 40 pt |
| macOS `muda` 原生菜单栏 | 不安装；窗口内菜单 + 长按命令面板 |
| `--demo` | 没有崩溃恢复可提供时，启动即 `file.openDemoProject`（Help ▸ Open Demo Project 是同一条命令），手机一开就有真实合成可看 |

其他要点：

- **CJK 字体**：EffectCraft 的主题会自己把「文字引擎找到的系统字体」追加进每个 egui 字体族
  （`theme::install` 的 `japanese-system`），但它的字体扫描器只看桌面字体目录
  （`/usr/share/fonts`、`$HOME/.fonts`），**不看 Android 的 `/system/fonts`**。所以启动时把
  `/system/fonts/NotoSansCJK-Regular.ttc` 复制进 `$HOME/.fonts/` 并触发一次扫描——上游代码自己
  完成剩下的事（UI 与文字工具同时受益）。`TouchShell::ensure_system_font` 是第二道保险：主题字体
  生效之后读-改-写 `ctx.fonts(|f| f.definitions().clone())`，把 CJK 字体追加到**每一个**字体族
  （包括 `FontFamily::Name("semibold")` / `("medium")`），语言/主题重建字体后会补回。
- **触屏命令面板**：顶部 8% 长按 500 ms 打开（桌面菜单条的小条目手指点不到）。按钮直接投
  EffectCraft 自己的命令 id；需要弹文件选择器的（`file.import`、`file.open`、`file.save`、
  `file.saveAs`）走 `ui.menu.invoke`（应用自己的菜单分发器，它调 `Hooks` 再执行同名引擎命令），
  其余走 `engine.execute`：`app.newComp`、`file.openDemoProject`、`app.commandPalette`、
  `renderQueue.add`、`app.home`、`app.settings`、`app.about`、`window.workspace {"name":"Phone"}`、`app.quit`。
- **GPU**：窗口由 eframe/wgpu 画（Vulkan/SwiftShader），合成器的 GPU 计算走应用自己的
  `effectcraft-gpu`；桌面那份 `GpuFailureBridge`（未捕获 GPU 错误 / 设备丢失 → UI 报告）原样装上了，
  失败会退回 CPU 渲染而不是静默崩溃。
- **启动自检**（`src/selftest.rs`，后台线程，从不 panic）：内嵌 `assets/test.mp4`（320×240 H.264，
  25 fps）与 `assets/test.png`，经 **EffectCraft 自己的媒体层**（`effectcraft-media::MediaPool` →
  FilmCraft 的纯 Rust 编解码）逐帧解码并采样亮度；再用引擎自己的命令 `file.import` → `comp.new` →
  `layer.addItem` → `session.render(...)` 渲染一帧并数不透明像素。一行 logcat 证明「探测 → 解码 →
  工程条目 → 图层 → 渲染器」整条脊梁在设备上工作。

## 3. Gradle 外壳

`android/` 是一份可直接用的 Gradle 工程（AGP 9.1.0 / Gradle 9.3.1 / JDK 21）：

| 位置 | 值 |
|---|---|
| `app/build.gradle` → `namespace` / `applicationId` | `ai.storyteller.effectcraft` |
| `app/build.gradle` → `versionName` | `0.4.0`（= 上游 workspace 版本） |
| `AndroidManifest.xml` → `android:label` | `EffectCraft` |
| `MainActivity.java` / `SafBridge.java` 包名 | `ai.storyteller.effectcraft` |
| `android.app.lib_name` | `main`（与本 crate `[lib] name = "main"` 一致） |
| 启动图标 | 上游自己的 `assets/app-icon/effectcraft-1024.png` 缩放成 mipmap（ArtCraft 自有素材，MIT OR Apache-2.0；不是 Adobe 素材） |

依赖 `androidx.games:games-activity:4.4.0`（**GameActivity，不是 NativeActivity**：NativeActivity 没有
IME，而且 eframe 的 `accesskit` + `native-activity` 组合是硬 compile error——`effectcraft-ui-egui`
本身强制开了 `accesskit`）。

## 4. 构建（照抄）

```bash
export PATH="$HOME/.cargo/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/28.2.13676358"
export JAVA_HOME=/opt/homebrew/opt/openjdk@21

~/artcraft-mobile/scripts/build-app.sh effectcraft      # 内部走构建锁，自动选 --release
# → effectcraft/apps/effectcraft-android/jniLibs/arm64-v8a/libmain.so

cd effectcraft/apps/effectcraft-android/android
~/artcraft-mobile/tools/gradle-9.3.1/bin/gradle assembleRelease --no-daemon -Dhttp.nonProxyHosts='*'
# → app/build/outputs/apk/release/app-release.apk
```

（`gradlew` 用不了：`~/.gradle/gradle.properties` 里写死了代理，wrapper 下载不到发行版——见
`HANDOFF.md` §5.2。）

安装、启动、看日志（模拟器用 `-s emulator-5554`，真机用 `adb devices -l` 里的序列号）：

```bash
ADB="$ANDROID_HOME/platform-tools/adb"
APK=apps/effectcraft-android/android/app/build/outputs/apk/release/app-release.apk
"$ADB" -s "$SERIAL" install -r "$APK"
"$ADB" -s "$SERIAL" shell am start -n ai.storyteller.effectcraft/.MainActivity
"$ADB" -s "$SERIAL" logcat | grep -Ei "effectcraft|SafBridge|panic|AndroidRuntime|wgpu"
```

或直接用仓库里的冒烟脚本：

```bash
~/artcraft-mobile/scripts/device-smoke.sh ai.storyteller.effectcraft .MainActivity EffectCraft auto effectcraft-real
```

成功的样子（logcat）：`android_main entered` → `HOME → /data/user/0/ai.storyteller.effectcraft/files` →
`system CJK font NotoSansCJK-Regular.ttc → …` → `touch mode applied` → `app created; the window shows
after its first frame` → `素材自检: test.mp4 H.264 25 fps … ／ 引擎自检: file.import → 素材 … → 合成
320x240 → 渲染 …，N 个不透明像素`。

## 5. 已知缺口（首版必然缺的东西）

- **音频**：`audio_device` / `audio_devices` 未接（预览静音）。接 cpal 的 AAudio 后端还需要
  `RECORD_AUDIO` 之外的输出路由与权限处理。
- **导出落点**：渲染队列导出的目标来自 `pick_folder`（固定 exports 目录）；工程/帧的「另存为」
  已经能经 `ACTION_CREATE_DOCUMENT` 发布到用户选的位置。下一步：接 `ACTION_OPEN_DOCUMENT_TREE`，
  让导出目录也能由用户挑。
- **目录选择器**：同上，`pick_folder` 不弹选择器。
- **剪贴板 / 外部编辑器**：`clipboard_text`、`app_action` 未接（桌面只在 macOS 用）。
- **触屏手势**：目前只有长按面板 + 40 pt 命中目标；拖动图层、时间线缩放、双指缩放等桌面鼠标
  交互还没有触摸等价物（FilmCraft 试点用 `dragged()` / `zoom_delta()` / `double_clicked()` 做过探针，
  可按需接）。
- **工作区持久化**：`UiState`（含 dock）由应用自己管理，Android 侧不额外写文件；每次启动都会先装
  "Phone" 工作区（用户随后在 Window ▸ Workspace 里的选择当次有效）。
- **真机 vs 模拟器**：模拟器需要 `-no-window -gpu swiftshader_indirect`（Apple Silicon 上
  Vulkan/MoltenVK 不稳，见 `HANDOFF.md` §5.3）；GPU 合成在 SwiftShader 上可能回退 CPU。
