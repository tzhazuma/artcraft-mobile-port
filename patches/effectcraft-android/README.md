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

## 5. 验证（本次实测）

真机 **vivo PA2573 / Android 16**（`adb connect 192.168.0.106:37379`）与模拟器
**emulator-5554**（Medium_Phone_API_36.0，无头 + SwiftShader）都装得上、起得来、无 panic：

```
# 真机（Mali-G925-Immortalis MC12，走 GPU 合成）
effectcraft-android: android_main entered
effectcraft-android: HOME → /data/user/0/ai.storyteller.effectcraft/files
effectcraft-android: system CJK font NotoSansCJK-Regular.ttc → …/files/.fonts/NotoSansCJK-Regular.ttc (32355424 bytes)
effectcraft-android: 素材自检: test.mp4 H.264 (Baseline) 25 fps，时长 1.00s → 解出 25 帧（320x240），采样亮度 0.393
                      ／ 静帧 test.png 64x64（PNG） ／ 引擎自检: file.import → 素材 1 → 合成 320x240 → 渲染 320x240，76800 个不透明像素
effectcraft-android: graphics adapter AdapterInfo { name: "Mali-G925-Immortalis MC12", device_type: IntegratedGpu, backend: Vulkan }
effectcraft_ui_egui: GPU compositor: Mali-G925-Immortalis MC12 (Vulkan)
effectcraft-android: touch mode applied (ppp 2.5, logical 1238×826 pt, interact 40 pt)
ActivityTaskManager: Displayed ai.storyteller.effectcraft/.MainActivity for user 0: +201ms
```

真机截图 `shots/effectcraft-real.png`：Phone 工作区里 **Composition: EffectCraft Intro** 正渲染示例工程
的片头（EFFECTCRAFT 标题卡），下面是 Timeline（8 个图层、时间标尺、关键帧条），状态栏
`Frame Render Time 78 ms`——GPU 合成在真机上工作。

```
# 模拟器（SwiftShader = CPU 光栅化器：合成退回 CPU，见下）
effectcraft-android: SwiftShader Device (LLVM 10.0.0) is a software rasterizer; compositing on the CPU
effectcraft_ui_egui: GPU preview retired: software rasterizer (CPU adapter): compositing on the CPU
effectcraft-android: touch mode applied (ppp 2.625, logical 411×914 pt, interact 40 pt)
```

模拟器截图 `shots/effectcraft-emulator.png`（竖屏 411×914 pt 的 Phone 工作区 + 示例工程渲染）与
`shots/effectcraft-smoke.png`（`scripts/device-smoke.sh` 的 PASS 截图）。

体积：release `.so` **76.3 MB**、APK **68.1 MB**（`dist/EffectCraft-0.4.0-android-arm64.apk`，
sha256 `f06d847350eb6385df850b4c11ea1c70f6ffb99a478ba7357244912f7b7b26ed`），16 KB 页对齐
（`zipalign -c -P 16` 通过）。比 FilmCraft/LightCraft 大是应用本身大：0.4.0 带四个纯 Rust 编码器
（AV1/HEVC/VP9/Opus）、wasmi 插件运行时、boa JS 引擎、HarfBuzz 移植的文字排版，以及 PDF/PSD/SVG/
Lottie/glTF 导入器。要更小可给 release 加 `strip = "symbols"`（本次没改上游 profile）。

**模拟器的 CPU 光栅化回退**：`android_main` 里检查适配器 `device_type == Cpu`（SwiftShader、
llvmpipe），是的话先向 `GpuFailureBridge` 报一次失败——应用自己就会用 CPU 合成（它本来就是设备跑不了
compute 流水线时的路径）。不加这一步，模拟器上创建合成器设备 + 编译 compute 管线要 **约 3 分钟**
才出第一帧（真机 200 ms）。真机适配器是 `IntegratedGpu`，不会触发，GPU 合成照常。

**崩溃恢复 vs 示例工程**：没有可恢复的自动保存时启动即开示例工程；如果上一次没有干净退出
（`begin_recovery()` 返回 Some），则按上游行为弹出恢复提示、**不**打开示例工程——此时 Composition
面板是空的（`New Composition`），这是对的，不要用示例工程盖掉用户的自动保存。

## 6. 已知缺口（首版必然缺的东西）

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
