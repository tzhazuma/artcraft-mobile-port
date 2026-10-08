# crates/platform 的 MediaCodec 硬解后端（Android）

把这件事补齐：上游 `crates/platform` 只有 macOS 的 VideoToolbox，Android 上 `register()`
一直返回 `Unavailable`、只能软解。这个补丁给 Android 接了 **MediaCodec（AMediaCodec）** 后端。

## 文件

| 文件 | 作用 |
|---|---|
| `src/mediacodec.rs` | `MediaCodecDecoder`：创建/配置 AVC/HEVC 解码器、avcC/hvcC→Annex-B 转换、输出缓冲→`VideoFrame`（8-bit NV12/I420 与 10-bit P010 的去交织、裁剪、16-bit 采样换算） |
| `src/lib.rs` | `register()` / `registered()` / `hardware_decoder_for()` 加 Android 分支；新增 `mediacodec_factory`（与 `videotoolbox_factory` 同形，外面套 `HybridDecoder` 提供中途回退） |
| `Cargo.toml` | 新增 `[target.'cfg(target_os = "android")'.dependencies]`（`ndk` 的 `media` + `api-level-31`） |

接入方式：把三个文件覆盖到 filmcraft 仓库的 `crates/platform/` 对应路径即可。

## 设计要点

- **拒绝而不是硬撑**：非 H.264/HEVC、>10 bit、luma/chroma 位深不一致、非 4:2:0、隔行、缺 SPS/PPS，
  以及设备上该编码器是软件解码器（`c2.android.*` / `*.sw.*`，用它只会比自带软解更慢）——都返回 `Err`
  让引擎走软解，并按上游约定调用 `note_hw_declined()`。
- **回退免费**：工厂返回 `HybridDecoder::new(Box::new(MediaCodecDecoder), entry, info)`，中途失败
  由上游现成的机制接管（自动切软解 + 重放），计数器 `note_hw_fallback()` 也是现成的。
- **帧转换**：从 `output_format()` 读 `color-format` / `stride` / `slice-height` / `crop`，
  按行拷贝出紧凑的 YUV420 三平面（半平面 NV12 顺手去交织成 U/V 两平面）；设备给了 crop 矩形就以它为准，
  没有就用 SPS 里的 `info.crop`。**注意 MediaCodec 的 `crop-right` / `crop-bottom` 是"包含在内的
  最后一列/行"**（`MediaFormat` 与 `MediaCodec` 的 crop 说明：等于宽度-1 / 高度-1），所以画面尺寸是
  `right - left + 1` × `bottom - top + 1`；按差值算会丢最后一列/一行（320×240 的流解出 319×239），
  自检里与软解的逐像素对比会立刻暴露这一点。
  10-bit 流请求 P010（`0x36`），输出为 16-bit 词、10 bit 在高位：按行取 `u16`（LE）右移 6 位得到
  `0..=1023` 的 10-bit code，写进 `PixelData::Yuv16 { bits: 10 }` —— 与 `videotoolbox.rs::copy_out`
  的输出约定一致（`stride` 按字节给，少数厂商按像素给，用缓冲区大小区分；色度行距先按 luma 行距、
  不够再折半）。设备若直接给右对齐的 code（图中没有任何 >1023 的值、低位有非零）则不右移，并在日志里说明。
- **零业务 unsafe**：`ndk` crate 包好了 FFI；本模块唯一的 `unsafe` 是 `SendCodec` 的
  `unsafe impl Send`（`AMediaCodec` 无线程亲和性、`&mut self` 保证不会并发访问），并且按
  ADR 0001 放在 FFI 模块里、带 `// SAFETY:` 说明。

## 验证（真机 + 模拟器，`ai.storyteller.filmcraft` release APK）

App 启动时跑一次内嵌片段的自检（`patches/filmcraft-android/src/selftest.rs`：H.264、HEVC、
HEVC Main 10 三段 320×240 的流），走的都是**引擎自己的 `make_video_decoder()` 路径**
（工厂 → HybridDecoder → MediaCodec），`hw_stats` 计数同步增长。

真机（vivo PA2573 / MTK MT6991，Android 16）：

```
I/filmcraft_platform::mediacodec: MediaCodec: decoding 320x240 H.264 with c2.mtk.avc.decoder
I/filmcraft_platform::mediacodec: MediaCodec: output color-format 0x15, stride 320, slice-height 240, picture 320x240 at (0,0), 8-bit samples, 320-byte rows
I/filmcraft_platform::mediacodec: MediaCodec: decoding 320x240 HEVC with c2.mtk.hevc.decoder
I/filmcraft_platform::mediacodec: MediaCodec: output color-format 0x15, stride 320, slice-height 240, picture 320x240 at (0,0), 8-bit samples, 320-byte rows
I/filmcraft_platform::mediacodec: MediaCodec: decoding 320x240 HEVC with c2.mtk.hevc.decoder
I/filmcraft_platform::mediacodec: MediaCodec: output color-format 0x36 (P010), stride 640, slice-height 240, picture 320x240 at (0,0), 10-bit samples, 640-byte rows
I/main: filmcraft-android: H.264 自检: 解码器 MediaCodec H.264 | 样本 30 个 → 解出 30 帧（320x240） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0 ／ HEVC 自检: 解码器 MediaCodec HEVC | 样本 30 个 → 解出 30 帧（320x240） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0 ／ HEVC 10-bit 自检: 解码器 MediaCodec HEVC | 样本 30 个 → 解出 30 帧（320x240） | 硬件计数 帧 30 会话 1 拒绝 0 回退 0 | 与软解对比: 30 帧与软解逐像素一致
```

即：MTK 硬解把 Main 10 也接了下来（请求 P010、`stride` 640 字节 = 320 样本 × 2），30 帧与
FilmCraft 自己的软解**逐像素一致**——16-bit→10-bit 的换算与软解完全对齐。

模拟器（`c2.goldfish.*`，只声明 Main/MainStill、没有 P010）：8-bit 两段正常硬解，10-bit 那段
`c2.goldfish.hevc.decoder` 在 configure 后第一个样本就报 `ErrorUnknown`，`HybridDecoder` 按设计
切软解并解出全部 30 帧（`帧 0 会话 1 拒绝 0 回退 1`），与软解一致——即"模拟器不支持 Main10"也是
一条正常结果，引擎不会因此解不出画面。

## 已知限制 / 下一步

- 支持 **H.264 与 HEVC（8-bit / 10-bit，4:2:0，渐进）**：H.264 用 `csd-0`=SPS、`csd-1`=PPS；
  HEVC 用 `csd-0`=VPS+SPS+PPS（都按 Annex-B 打包）。4:2:2（以及 4:4:4、单色、隔行）仍被拒绝，
  交回 FilmCraft 自己的软解。
  - 4:2:2 在这台真机（MTK MT6991）上**没有硬件路径**：`dumpsys media.player` 里
    `c2.mtk.hevc.decoder` 只声明 `Main / MainStill / Main10 / Main10HDR10 / Main10HDR10Plus` 这些
    4:2:0 profile，输出颜色格式也只有 YUV420 系 + `0x36 (YUVP010)`，整个设备列表里没有任何
    RExt / 4:2:2 profile 或 P210 之类的 4:2:2 格式。所以拒绝 4:2:2 就是正确行为，
    真要支持得先换一台声明了 4:2:2 的设备。
- `color-format` 处理 19（I420）、21/`flexible`（NV12 系）与 54（P010，10-bit 流只请求它）；
  10-bit 缓冲为 16-bit 词（LE），按 `>> 6` 取 10-bit code（右对齐的少数实现不右移）；遇到别的值按
  NV12/P010 处理并告警。
- 模拟器上 `c2.goldfish.*` 是模拟器自己的解码器（只声明 Main/MainStill，**不支持 Main10**）；
  真机上是厂商解码器（如 MTK `c2.mtk.hevc.decoder`，声明 Main10/Main10HDR10）。
- 尚未接 `create_input_surface`（零拷贝上屏）——当前走字节缓冲，够用且简单。
