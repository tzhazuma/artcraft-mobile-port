# crates/platform 的 MediaCodec 硬解后端（Android）

把这件事补齐：上游 `crates/platform` 只有 macOS 的 VideoToolbox，Android 上 `register()`
一直返回 `Unavailable`、只能软解。这个补丁给 Android 接了 **MediaCodec（AMediaCodec）** 后端。

## 文件

| 文件 | 作用 |
|---|---|
| `src/mediacodec.rs` | `MediaCodecDecoder`：创建/配置 AVC 解码器、avcC→Annex-B 转换、输出缓冲→`VideoFrame`（含 NV12 去交织与裁剪）、EOS 排空 |
| `src/lib.rs` | `register()` / `registered()` / `hardware_decoder_for()` 加 Android 分支；新增 `mediacodec_factory`（与 `videotoolbox_factory` 同形，外面套 `HybridDecoder` 提供中途回退） |
| `Cargo.toml` | 新增 `[target.'cfg(target_os = "android")'.dependencies]`（`ndk` 的 `media` + `api-level-31`） |

接入方式：把三个文件覆盖到 filmcraft 仓库的 `crates/platform/` 对应路径即可。

## 设计要点

- **拒绝而不是硬撑**：非 H.264、>8 bit、非 4:2:0、隔行、缺 SPS/PPS，以及设备上 `video/avc`
  是软件解码器（`c2.android.*` / `*.sw.*`，用它只会比自带软解更慢）——都返回 `Err` 让引擎走软解，
  并按上游约定调用 `note_hw_declined()`。
- **回退免费**：工厂返回 `HybridDecoder::new(Box::new(MediaCodecDecoder), entry, info)`，中途失败
  由上游现成的机制接管（自动切软解 + 重放），计数器 `note_hw_fallback()` 也是现成的。
- **帧转换**：从 `output_format()` 读 `color-format` / `stride` / `slice-height` / `crop`，
  按行拷贝出紧凑的 YUV420 三平面（半平面 NV12 顺手去交织成 U/V 两平面），
  与 `videotoolbox.rs::copy_out` 的输出约定一致。
- **零业务 unsafe**：`ndk` crate 包好了 FFI；本模块唯一的 `unsafe` 是 `SendCodec` 的
  `unsafe impl Send`（`AMediaCodec` 无线程亲和性、`&mut self` 保证不会并发访问），并且按
  ADR 0001 放在 FFI 模块里、带 `// SAFETY:` 说明。

## 验证（模拟器，`ai.storyteller.filmcraft` release APK）

App 启动时跑一次内嵌片段的自检（`patches/filmcraft-android/src/selftest.rs`），logcat：

```
I filmcraft_platform::mediacodec: MediaCodec: decoding 320x240 H.264 with c2.goldfish.h264.decoder
I filmcraft_platform::mediacodec: MediaCodec: output color-format 0x15, stride 320, slice-height 240, picture 319x239 at (0,0)
I main: filmcraft-android: 自检: 解码器 MediaCodec H.264 | 样本 30 个 → 解出 30 帧（319x239） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0
```

即：**引擎自己的 `make_video_decoder()` 路径**（工厂 → HybridDecoder → MediaCodec）把 30 个样本
全部解出，`hw_stats` 计数同步增长，无回退。

## 已知限制 / 下一步

- 只支持 **H.264 8-bit 4:2:0**；HEVC（`hvcC`，`video/hevc`）与 10-bit 是自然的下一步。
- `color-format` 只处理 19（I420）与 21/`flexible`（NV12 系）；遇到别的值会按 NV12 处理并告警。
- 模拟器上 `c2.goldfish.h264.decoder` 是模拟器自己的解码器；真机上是厂商解码器（效果更明显）。
- 尚未接 `create_input_surface`（零拷贝上屏）——当前走字节缓冲，够用且简单。
