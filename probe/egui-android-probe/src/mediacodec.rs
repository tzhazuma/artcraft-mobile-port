//! MediaCodec（AMediaCodec）硬解探针：解码内嵌的 H.264 Annex-B 测试流。
//!
//! 这条路径对应 filmcraft 上游 `crates/platform`（macOS 用 VideoToolbox）在 Android 上的对应物：
//! 上游只实现了 VideoToolbox，Android 需要 MediaCodec 后端；这里先用最小代码验证
//! 「AMediaCodec 能在设备上创建解码器 → 送 Annex-B 数据 → 吐出解码帧」这条链路是通的。
#![cfg(target_os = "android")]

use std::time::{Duration, Instant};

use ndk::media::media_codec::{
    DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection,
};
use ndk::media::media_format::MediaFormat;

/// ffmpeg 生成：320x240、30fps、1 秒、baseline profile、Annex-B 字节流
const TEST_H264: &[u8] = include_bytes!("../assets/test.h264");

#[derive(Debug, Default)]
pub struct DecodeReport {
    pub codec: String,
    pub decoder_created: bool,
    pub configured: bool,
    pub started: bool,
    pub input_bytes: usize,
    pub frames: usize,
    pub out_format: String,
    pub error: Option<String>,
}

impl DecodeReport {
    pub fn summary(&self) -> String {
        let mut s = format!(
            "解码器: {}\n创建 {} / configure {} / start {}\n送入 {} 字节，解出 {} 帧",
            self.codec,
            yn(self.decoder_created),
            yn(self.configured),
            yn(self.started),
            self.input_bytes,
            self.frames
        );
        if !self.out_format.is_empty() {
            s.push_str(&format!("\n输出格式: {}", self.out_format));
        }
        if let Some(e) = &self.error {
            s.push_str(&format!("\n错误: {e}"));
        }
        s
    }
}

fn yn(b: bool) -> &'static str {
    if b { "✓" } else { "✗" }
}

pub fn run_decode_test() -> DecodeReport {
    let mut r = DecodeReport::default();

    let Some(codec) = MediaCodec::from_decoder_type("video/avc") else {
        r.error = Some("设备上没有 video/avc 解码器".into());
        return r;
    };
    r.decoder_created = true;
    r.codec = codec.name().unwrap_or_else(|_| "<unknown>".into());

    let mut fmt = MediaFormat::new();
    fmt.set_str("mime", "video/avc");
    fmt.set_i32("width", 320);
    fmt.set_i32("height", 240);
    if let Err(e) = codec.configure(&fmt, None, MediaCodecDirection::Decoder) {
        r.error = Some(format!("configure: {e}"));
        return r;
    }
    r.configured = true;
    if let Err(e) = codec.start() {
        r.error = Some(format!("start: {e}"));
        return r;
    }
    r.started = true;

    // ---- 送输入：按访问单元（AU）逐帧送（整段塞一个 buffer 时模拟器只吐 1 帧）----
    fn split_access_units(stream: &[u8]) -> Vec<&[u8]> {
        // 收集 Annex-B 起始码位置
        let mut starts: Vec<(usize, usize)> = Vec::new(); // (起始码偏移, NAL 头偏移)
        let n = stream.len();
        let mut i = 0;
        while i + 3 < n {
            if stream[i] == 0 && stream[i + 1] == 0 && stream[i + 2] == 1 {
                starts.push((i, i + 3));
                i += 3;
            } else if i + 4 < n && stream[i..i + 4] == [0, 0, 0, 1] {
                starts.push((i, i + 4));
                i += 4;
            } else {
                i += 1;
            }
        }
        if starts.is_empty() {
            return vec![stream];
        }
        let mut aus = Vec::new();
        let mut au_start = starts[0].0;
        let mut seen_vcl = false;
        for &(sc_off, hdr_off) in &starts {
            let nal_type = stream[hdr_off] & 0x1f;
            let is_vcl = nal_type == 1 || nal_type == 5;
            if is_vcl && seen_vcl {
                aus.push(&stream[au_start..sc_off]);
                au_start = sc_off;
                seen_vcl = false;
            }
            if is_vcl {
                seen_vcl = true;
            }
        }
        aus.push(&stream[au_start..]);
        aus
    }

    let aus = split_access_units(TEST_H264);
    let mut pts: u64 = 0;
    let in_deadline = Instant::now() + Duration::from_secs(15);
    'outer: for au in &aus {
        let mut off = 0usize;
        while off < au.len() && Instant::now() < in_deadline {
            match codec.dequeue_input_buffer(Duration::from_millis(500)) {
                Ok(DequeuedInputBufferResult::Buffer(mut ib)) => {
                    let buf = ib.buffer_mut();
                    let n = buf.len().min(au.len() - off);
                    for (dst, src) in buf.iter_mut().zip(&au[off..off + n]) {
                        dst.write(*src);
                    }
                    if let Err(e) = codec.queue_input_buffer(ib, 0, n, pts, 0) {
                        r.error = Some(format!("queue_input_buffer: {e}"));
                        break 'outer;
                    }
                    off += n;
                    r.input_bytes += n;
                }
                Ok(DequeuedInputBufferResult::TryAgainLater) => {}
                Err(e) => {
                    r.error = Some(format!("dequeue_input_buffer: {e}"));
                    break 'outer;
                }
            }
        }
        pts += 33_333;
    }
    let _ = codec.set_signal_end_of_input_stream();

    // ---- 收输出：一直排空到 EOS（BUFFER_FLAG_END_OF_STREAM）或超时 ----
    const BUFFER_FLAG_END_OF_STREAM: u32 = 4;
    let out_deadline = Instant::now() + Duration::from_secs(20);
    let mut eos = false;
    while !eos && Instant::now() < out_deadline {
        match codec.dequeue_output_buffer(Duration::from_millis(300)) {
            Ok(DequeuedOutputBufferInfoResult::Buffer(buf)) => {
                r.frames += 1;
                let flags = buf.info().flags();
                if r.frames == 1 {
                    let f = buf.format();
                    r.out_format = format!(
                        "{}x{}",
                        f.i32("width").unwrap_or(0),
                        f.i32("height").unwrap_or(0)
                    );
                }
                let _ = codec.release_output_buffer(buf, false);
                if flags & BUFFER_FLAG_END_OF_STREAM != 0 {
                    eos = true;
                }
            }
            Ok(DequeuedOutputBufferInfoResult::OutputFormatChanged) => {
                let f = codec.output_format();
                r.out_format = format!(
                    "{}x{}",
                    f.i32("width").unwrap_or(0),
                    f.i32("height").unwrap_or(0)
                );
            }
            Ok(DequeuedOutputBufferInfoResult::OutputBuffersChanged) => {}
            Ok(DequeuedOutputBufferInfoResult::TryAgainLater) => {}
            Err(e) => {
                r.error = Some(format!("dequeue_output_buffer: {e}"));
                break;
            }
        }
    }
    let _ = codec.stop();
    r
}
