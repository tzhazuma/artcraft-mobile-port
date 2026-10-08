# dist/ —— 发布 APK

本目录存放 FilmCraft → Android 移植的可安装产物（APK 随 git 入库；`filmcraft/` 克隆与 `**/build/` 产物不入库）。
**源头是 `patches/`**——已验证的功能清单见 [HANDOFF.md](../HANDOFF.md) §2/§3 与 [EXPERIMENT.md](../EXPERIMENT.md) §5.10。

## 产物

| 文件 | 应用 / id | 版本 | ABI / minSdk | 大小 |
|---|---|---|---|---|
| `FilmCraft-0.2.1-android-arm64.apk` | FilmCraft `ai.storyteller.filmcraft` | 0.2.1 | arm64-v8a / 31 | 44,535,267 B |
| `EguiProbe-1.0-android-arm64.apk` | egui/wgpu 探针 `com.artcraft.eguiandroidprobe` | 1.0 | arm64-v8a / 31 | 17,717,983 B |

```
sha256 FilmCraft-0.2.1-android-arm64.apk = dc70a7472a6e104b91455407bcba351dfeb64d7c4f264d8727755e3937b504cb
sha256 EguiProbe-1.0-android-arm64.apk   = 2eab8ce0ec1812e2a1e27040ba1ecb8d239652f18dd1e28481039c46ca545f88
```

两个包都是 **debug 签名**（`CN=Android Debug`），所以任何设备上 `adb install -r` 都能直接覆盖安装。

- **FilmCraft 0.2.1**：完整应用——长按命令面板、SAF 导入/保存、MediaCodec 硬解（H.264 / HEVC / HEVC Main 10，10-bit 走 P010 → `PixelData::Yuv16`）、导出走应用自带的纯 Rust 编码器；启动自检解码三段内嵌素材（H.264 / HEVC / 10-bit HEVC）。
- **EguiProbe 1.0**：stage 3 的最小 eframe + wgpu + GameActivity 宿主，用来单独验证渲染链路，不带 FilmCraft 引擎。

## 构建命令

FilmCraft（在 `filmcraft/` 克隆根目录；补丁先经 `rsync` 镜像进去，见 HANDOFF.md §4）：

```bash
cargo +stable ndk -t arm64-v8a -o apps/filmcraft-android/jniLibs build --release -p filmcraft-android
cd apps/filmcraft-android/android && ~/artcraft-mobile/tools/gradle-9.3.1/bin/gradle assembleRelease --no-daemon -Dhttp.nonProxyHosts='*'
```

探针（`probe/egui-android-probe/`，release 产物）：

```bash
cargo +stable ndk -t arm64-v8a -o app/src/main/jniLibs build --release
~/artcraft-mobile/tools/gradle-9.3.1/bin/gradle assembleRelease --no-daemon -Dhttp.nonProxyHosts='*'
```

只发 release（debug `.so` 855 MB 且不出包）；Gradle wrapper 不可用（本机代理坑），必须用手动下载的 Gradle 9.3.1。

## 安装与验证

```bash
adb install -r FilmCraft-0.2.1-android-arm64.apk
adb shell am start -n ai.storyteller.filmcraft/.MainActivity
adb logcat | grep 自检        # 启动自检：三段素材各 30/30 帧（10-bit 另与软解逐像素比对）
```

**arm64-v8a 单 ABI、minSdk 31（Android 12+）**：x86_64 模拟器装不上。校验本目录拷贝与构建产物逐字节一致：

```bash
shasum -a 256 dist/FilmCraft-0.2.1-android-arm64.apk
# 应输出：dc70a7472a6e104b91455407bcba351dfeb64d7c4f264d8727755e3937b504cb
```

一句话索引：**改代码改 `patches/`**（构建前镜像进克隆，HANDOFF.md §4）；真机 / 模拟器命令与坑见 HANDOFF.md §4/§5，10-bit 硬解与真机导出证据见 EXPERIMENT.md §5.10。
