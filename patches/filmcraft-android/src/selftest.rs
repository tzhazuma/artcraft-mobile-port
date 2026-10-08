#![cfg(target_os = "android")]

//! Hardware-decoding self test: decode the embedded H.264 clip through whatever decoder the
//! registered factories produce (the MediaCodec backend from `crates/platform` on Android) and
//! report the decoder, the picture count and the hardware counters.

use filmcraft_codecs::hw;
use filmcraft_isobmff::{AvcConfig, CodecConfig, FourCc, SampleEntry, VideoParams};

/// ffmpeg-generated clip: 320x240, 30 fps, 1 s, baseline, Annex-B.
const CLIP: &[u8] = include_bytes!("../assets/test.h264");

/// NAL units as (start, end, type) with the start code skipped.
fn split_nals(stream: &[u8]) -> Vec<(usize, usize, u8)> {
    let mut starts: Vec<usize> = Vec::new();
    let n = stream.len();
    let mut i = 0;
    while i + 3 < n {
        if stream[i] == 0 && stream[i + 1] == 0 && stream[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else if i + 4 < n && stream[i..i + 4] == [0, 0, 0, 1] {
            starts.push(i + 4);
            i += 4;
        } else {
            i += 1;
        }
    }
    let mut nals = Vec::with_capacity(starts.len());
    for (idx, &start) in starts.iter().enumerate() {
        let end = starts.get(idx + 1).map_or(n, |&s| s.saturating_sub(3)).min(n);
        if start < end {
            nals.push((start, end, stream[start] & 0x1f));
        }
    }
    nals
}

/// Access units as length-prefixed (4-byte) samples, the way the engine feeds decoders.
fn access_units(stream: &[u8]) -> Vec<Vec<u8>> {
    let nals = split_nals(stream);
    let mut units: Vec<Vec<u8>> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut seen_vcl = false;
    for (start, end, nal_type) in nals {
        let is_vcl = nal_type == 1 || nal_type == 5;
        if is_vcl && seen_vcl {
            units.push(std::mem::take(&mut current));
            seen_vcl = false;
        }
        let len = (end - start) as u32;
        current.extend_from_slice(&len.to_be_bytes());
        current.extend_from_slice(&stream[start..end]);
        if is_vcl {
            seen_vcl = true;
        }
    }
    if !current.is_empty() {
        units.push(current);
    }
    units
}

/// Decode [`CLIP`] with the first decoder the registered factories offer and describe what happened.
pub fn run() -> String {
    let nals = split_nals(CLIP);
    let sps = nals.iter().find(|n| n.2 == 7).map(|n| CLIP[n.0..n.1].to_vec());
    let pps = nals.iter().find(|n| n.2 == 8).map(|n| CLIP[n.0..n.1].to_vec());
    let (Some(sps), Some(pps)) = (sps, pps) else {
        return "自检: 测试流里没有 SPS/PPS".into();
    };
    let entry = SampleEntry {
        format: FourCc::new(b"avc1"),
        data_reference_index: 1,
        codec: CodecConfig::Avc(AvcConfig {
            profile: sps.get(1).copied().unwrap_or(0x42),
            compatibility: sps.get(2).copied().unwrap_or(0),
            level: sps.get(3).copied().unwrap_or(0x1e),
            length_size: 4,
            sps: vec![sps],
            pps: vec![pps],
            ext: Vec::new(),
        }),
        video: Some(VideoParams { width: 320, height: 240, ..Default::default() }),
        audio: None,
        bitrate: None,
    };

    let before = hw::hw_stats();
    let mut decoder = match filmcraft_codecs::make_video_decoder(&entry) {
        Ok(d) => d,
        Err(e) => return format!("自检: 创建解码器失败: {e}"),
    };
    let name = decoder.name().to_string();
    let units = access_units(CLIP);
    let mut frames = 0usize;
    let mut size = String::new();
    let mut error = None;
    for (i, unit) in units.iter().enumerate() {
        match decoder.decode(unit, i as i64) {
            Ok(out) => {
                for f in &out {
                    if size.is_empty() {
                        size = format!("{}x{}", f.frame.width, f.frame.height);
                    }
                }
                frames += out.len();
            }
            Err(e) => {
                error = Some(e.to_string());
                break;
            }
        }
    }
    frames += decoder.flush().len();
    let after = hw::hw_stats();

    let mut report = format!(
        "自检: 解码器 {name} | 样本 {} 个 → 解出 {frames} 帧{} | 硬件计数 帧 {} 会话 {} 拒绝 {} 回退 {}",
        units.len(),
        if size.is_empty() { String::new() } else { format!("（{size}）") },
        after.frames - before.frames,
        after.sessions - before.sessions,
        after.declined - before.declined,
        after.fallbacks - before.fallbacks,
    );
    if let Some(e) = error {
        report.push_str(&format!(" | 错误: {e}"));
    }
    report
}
