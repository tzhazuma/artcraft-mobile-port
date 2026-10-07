# ArtCraft Mobile Port

把 [ArtCraft / Crafting Apps](https://github.com/storytold)（纯 Rust 重写的 Adobe 全家桶）带到移动端的移植笔记与实验代码：

- **macOS 安装**：LightCraft / PhotoCraft / FilmCraft / EffectCraft / PrintCraft 的安装步骤与校验方法
- **移植方案**：[PORTING.md](PORTING.md) —— iOS / Android / 鸿蒙（移动端 + PC）的完整步骤、命令与风险
- **实验记录**：[EXPERIMENT.md](EXPERIMENT.md) —— 把 FilmCraft 移植到 Android 的分阶段实验（wasm → 交叉编译 → 最小原生宿主 → 入口 crate 集成）

## 目录

| 路径 | 内容 |
|---|---|
| `PORTING.md` | 移植总方案（三条路线 × 四类目标平台） |
| `EXPERIMENT.md` | Android 实验逐阶段结果与日志 |
| `scripts/download-dmgs.sh` | 下载 5 个 macOS DMG + SHA256SUMS |
| `scripts/install-macos-apps.sh` | 校验 → 挂载 → 安装到 /Applications → 签名/公证校验 |
| `probe/egui-android-probe/` | 最小 eframe+wgpu Android 宿主（验证 FilmCraft 同款技术链路） |
| `patches/filmcraft-android/` | FilmCraft 的 Android 入口 crate（拉伸目标） |

## 上游与许可

上游仓库（`storytold/*`）为 `MIT OR Apache-2.0` 双许可（GitHub 页面显示 Apache-2.0）。本仓库中的移植代码为独立编写，遵循同样的宽松许可精神；引用上游代码/文档时请保留其署名（见各上游仓库 `NOTICE` / `ATTRIBUTION.md`）。

## 环境快照（本实验机器）

- macOS 27.2 / arm64，Xcode 27.0 + CLT 26.3
- Rust stable 1.99.0（rustup）+ Android NDK r28.2.13676358 + cargo-ndk
- 网络：`github.com` 直连不稳定，`raw.githubusercontent.com` 被重置；下载走 `gh-proxy.com` / `ghproxy.net` 镜像，git 走 HTTPS/SSH-443
