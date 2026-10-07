# FilmCraft → Android 实验记录

> 逐步填写。环境：macOS 27.2/arm64，Rust stable 1.99.0，Android SDK `~/Library/Android/sdk`（AVD `Medium_Phone_API_36.0`，arm64-v8a，Android 36），NDK r28.2.13676358，cargo-ndk。

## 阶段 0：wasm 版在 Android 跑通

- [ ] 下载 `filmcraft-web-0.2.1.zip` 并解压
- [ ] 本地静态服务器 + `adb reverse` + 模拟器/真机 Chrome 打开
- [ ] 记录 `window.filmcraft.info()` 的 backend/compositor/WebGPU 可用性

（待填）

## 阶段 1：Android 工具链准备

| 项 | 状态 | 备注 |
|---|---|---|
| Rust stable ≥1.95 | ✅ 1.99.0 | 初次 `rustup toolchain install stable` 因残留状态失败，重试成功 |
| Android targets（aarch64/x86_64-linux-android） | 进行中 | `rustup target add --toolchain stable` |
| cargo-ndk | 进行中 | `cargo +stable install cargo-ndk --locked` |
| NDK r28.2.13676358 | ✅ | `sdkmanager --sdk_root=$HOME/Library/Android/sdk`（原机只有 r26，16KB 页未对齐） |
| Java / adb / AVD | ✅ | Java 27；adb 36.0.1；AVD `Medium_Phone_API_36.0` |

## 阶段 2：filmcraft 交叉编译检查

（待填：`cargo check --target aarch64-linux-android` 对 L0–L4 与 UI shell 的结果）

## 阶段 3：最小 eframe+wgpu Android 宿主

（待填：probe 工程编译/安装/运行结果与 logcat）

## 阶段 4（拉伸）：filmcraft Android 入口 crate 集成

（待填：编译错误清单与修复进展）
