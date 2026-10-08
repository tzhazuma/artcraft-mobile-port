#![cfg(target_os = "android")]

//! Hardware-decoding self test: decode the embedded H.264 and HEVC clips through whatever decoders
//! the registered factories produce (the MediaCodec backend from `crates/platform` on Android) and
//! report each decoder, its picture count and the hardware counters.

use filmcraft_codecs::hw;
use filmcraft_isobmff::{AvcConfig, CodecConfig, FourCc, HevcConfig, HevcNalArray, SampleEntry, VideoParams};

/// ffmpeg-generated clips: 320x240, 30 fps, 1 s, Annex-B.
const CLIP_H264: &[u8] = include_bytes!("../assets/test.h264");
const CLIP_HEVC: &[u8] = include_bytes!("../assets/test.hevc");

/// NAL units as (start, end, type) with the start code skipped. HEVC packs the type in the high
/// bits of the first byte of a two-byte header, H.264 in the low five bits of one byte.
fn split_nals(stream: &[u8], hevc: bool) -> Vec<(usize, usize, u8)> {
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
            let byte = stream[start];
            let nal_type = if hevc { (byte >> 1) & 0x3f } else { byte & 0x1f };
            nals.push((start, end, nal_type));
        }
    }
    nals
}

/// VCL NAL types: H.264 1/5, HEVC 0–31.
fn is_vcl(nal_type: u8, hevc: bool) -> bool {
    if hevc { nal_type <= 31 } else { nal_type == 1 || nal_type == 5 }
}

/// Access units as length-prefixed (4-byte) samples, the way the engine feeds decoders.
fn access_units(stream: &[u8], hevc: bool) -> Vec<Vec<u8>> {
    let mut units: Vec<Vec<u8>> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut seen_vcl = false;
    for (start, end, nal_type) in split_nals(stream, hevc) {
        let vcl = is_vcl(nal_type, hevc);
        if vcl && seen_vcl {
            units.push(std::mem::take(&mut current));
            seen_vcl = false;
        }
        let len = (end - start) as u32;
        current.extend_from_slice(&len.to_be_bytes());
        current.extend_from_slice(&stream[start..end]);
        if vcl {
            seen_vcl = true;
        }
    }
    if !current.is_empty() {
        units.push(current);
    }
    units
}

/// A `320x240` visual sample entry for the given configuration.
fn entry_for(codec: CodecConfig) -> SampleEntry {
    SampleEntry {
        format: FourCc::new(b"avc1"),
        data_reference_index: 1,
        codec,
        video: Some(VideoParams { width: 320, height: 240, ..Default::default() }),
        audio: None,
        bitrate: None,
    }
}

/// Decode `clip` with the first decoder the registered factories offer and describe what happened.
fn run_one(label: &str, clip: &[u8], hevc: bool) -> String {
    let nals = split_nals(clip, hevc);
    let find = |t: u8| nals.iter().find(|n| n.2 == t).map(|n| clip[n.0..n.1].to_vec());
    let entry = if hevc {
        let (Some(vps), Some(sps), Some(pps)) = (find(32), find(33), find(34)) else {
            return format!("{label} 自检: 测试流缺少 VPS/SPS/PPS");
        };
        let array = |nal_type: u8, nalu: Vec<u8>| HevcNalArray { completeness: true, nal_type, nalus: vec![nalu] };
        entry_for(CodecConfig::Hevc(HevcConfig {
            general_profile_idc: 1,
            general_level_idc: 120,
            chroma_format_idc: 1,
            bit_depth_luma: 8,
            bit_depth_chroma: 8,
            length_size: 4,
            arrays: vec![array(32, vps), array(33, sps), array(34, pps)],
            ..Default::default()
        }))
    } else {
        let (Some(sps), Some(pps)) = (find(7), find(8)) else {
            return format!("{label} 自检: 测试流缺少 SPS/PPS");
        };
        entry_for(CodecConfig::Avc(AvcConfig {
            profile: sps.get(1).copied().unwrap_or(0x42),
            compatibility: sps.get(2).copied().unwrap_or(0),
            level: sps.get(3).copied().unwrap_or(0x1e),
            length_size: 4,
            sps: vec![sps],
            pps: vec![pps],
            ext: Vec::new(),
        }))
    };

    let before = hw::hw_stats();
    let mut decoder = match filmcraft_codecs::make_video_decoder(&entry) {
        Ok(d) => d,
        Err(e) => return format!("{label} 自检: 创建解码器失败: {e}"),
    };
    let name = decoder.name().to_string();
    let units = access_units(clip, hevc);
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
        "{label} 自检: 解码器 {name} | 样本 {} 个 → 解出 {frames} 帧{} | 硬件计数 帧 {} 会话 {} 拒绝 {} 回退 {}",
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

/// Decode both embedded clips; called once at startup from `android_main`.
pub fn run() -> String {
    let mut out = run_one("H.264", CLIP_H264, false);
    out.push_str(" ／ ");
    out.push_str(&run_one("HEVC", CLIP_HEVC, true));
    out
}
