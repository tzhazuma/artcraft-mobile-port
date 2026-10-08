//! MediaCodec (Android) hardware video decoding — the counterpart of [`crate::videotoolbox`].
//!
//! The AVC decoder is created through `AMediaCodec` (the safe `ndk` bindings), configured from the
//! sample entry's parameter sets, fed Annex-B access units and drained into the same planar
//! [`VideoFrame`]s the software decoder produces. The [`crate::HybridDecoder`] wrapper supplies the
//! mid-stream software fallback, so this type only has to be a faithful hardware decoder.
//!
//! Streams this backend does not take are declined in [`MediaCodecDecoder::new`] (which makes the
//! caller fall through to FilmCraft's own decoder): anything but 8-bit 4:2:0 progressive H.264, and
//! devices whose `video/avc` decoder is a software one (`c2.android.*` / `*.sw.*`) — using those
//! would only be slower than our own decoder while reporting itself as "hardware".
//!
//! This module holds the crate's only `unsafe`: the `Send` impl for the MediaCodec handle
//! ([`SendCodec`]), which the engine needs because [`VideoDecoder`] is `Send`.

use std::time::{Duration, Instant};

use filmcraft_codecs::hw::{NalCodec, NalStreamInfo};
use filmcraft_codecs::{CodecError, DecodedFrame, Result, VideoDecoder};
use filmcraft_frame::{Chroma, PixelData, VideoFrame, pool};
use filmcraft_time::Tick;
use ndk::media::media_codec::{DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection};
use ndk::media::media_format::MediaFormat;

/// `AMEDIACODEC_BUFFER_FLAG_CODEC_CONFIG`: the buffer holds codec setup data, not a picture.
const BUFFER_FLAG_CODEC_CONFIG: u32 = 2;
/// `AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM`.
const BUFFER_FLAG_END_OF_STREAM: u32 = 4;

/// `AMEDIAFORMAT_KEY_COLOR_FORMAT` values we understand.
const COLOR_FORMAT_YUV420_PLANAR: i32 = 19;
const COLOR_FORMAT_YUV420_SEMI_PLANAR: i32 = 21;

/// How long to wait for an input buffer before giving up on a sample.
const INPUT_TIMEOUT: Duration = Duration::from_millis(500);
/// How long to keep draining output buffers at end of stream.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// `AMediaCodec` has no thread affinity: it may be used from whichever thread holds it, just never
/// from two at once. [`VideoDecoder`] is `Send` (the engine moves decoders between frame workers
/// and only ever touches one through `&mut self`), and the `ndk` wrapper does not implement `Send`,
/// so this newtype carries it across threads.
struct SendCodec(MediaCodec);

// SAFETY: `MediaCodecDecoder` owns the handle outright and every method takes `&mut self`, so two
// threads can never touch the same codec; AMediaCodec itself is thread-agnostic as long as calls
// are not concurrent (Android NDK media API documentation).
#[allow(unsafe_code)]
unsafe impl Send for SendCodec {}

/// A hardware H.264 decoder built on `AMediaCodec`.
pub struct MediaCodecDecoder {
    codec: SendCodec,
    info: NalStreamInfo,
    /// Reused Annex-B conversion of the sample being fed.
    annexb: Vec<u8>,
    /// Whether the output layout has been logged (device formats differ).
    logged_layout: bool,
}

impl MediaCodecDecoder {
    /// Create and configure a decoder for `info`, or explain why this backend declines the stream.
    pub fn new(info: NalStreamInfo) -> std::result::Result<Self, String> {
        if info.codec != NalCodec::H264 {
            return Err("only H.264 is wired up".into());
        }
        if info.bit_depth_luma > 8 || info.bit_depth_chroma > 8 {
            return Err(format!("only 8-bit, got {}/{}", info.bit_depth_luma, info.bit_depth_chroma));
        }
        if info.chroma_format_idc != 1 {
            return Err(format!("only 4:2:0, got chroma_format_idc {}", info.chroma_format_idc));
        }
        if info.interlaced {
            return Err("interlaced streams are not supported".into());
        }
        if info.parameter_sets.len() < 2 {
            return Err("sample entry has no SPS/PPS".into());
        }

        let codec = MediaCodec::from_decoder_type("video/avc").ok_or("no video/avc decoder on this device")?;
        let name = codec.name().unwrap_or_else(|_| "MediaCodec H.264".to_string());
        if name.starts_with("c2.android.") || name.contains(".sw.") {
            return Err(format!("{name} is a software codec"));
        }

        let mut format = MediaFormat::new();
        format.set_str("mime", "video/avc");
        format.set_i32("width", info.crop.2 as i32);
        format.set_i32("height", info.crop.3 as i32);
        // Ask for semi-planar output; devices that ignore it report the real format in
        // `output_format()` and we read the layout from there.
        format.set_i32("color-format", COLOR_FORMAT_YUV420_SEMI_PLANAR);
        // csd-0 = SPS, csd-1 = PPS, both Annex-B (start-code prefixed) as MediaCodec expects.
        format.set_buffer("csd-0", &annexb_nals(std::slice::from_ref(&info.parameter_sets[0])));
        format.set_buffer("csd-1", &annexb_nals(std::slice::from_ref(&info.parameter_sets[1])));

        codec.configure(&format, None, MediaCodecDirection::Decoder).map_err(|e| format!("configure: {e}"))?;
        codec.start().map_err(|e| format!("start: {e}"))?;
        log::info!("MediaCodec: decoding {}x{} H.264 with {name}", info.crop.2, info.crop.3);
        Ok(Self { codec: SendCodec(codec), info, annexb: Vec::new(), logged_layout: false })
    }

    /// Feed one sample (avcC length-prefixed) into the decoder.
    fn feed(&mut self, sample: &[u8], pts: i64) -> Result<()> {
        self.annexb.clear();
        let length_size = self.info.length_size;
        if length_size == 0 || length_size > 4 {
            return Err(CodecError::Decode(format!("bad NAL length size {length_size}")));
        }
        let mut pos = 0usize;
        while pos + length_size <= sample.len() {
            let mut len = 0usize;
            for &b in &sample[pos..pos + length_size] {
                len = (len << 8) | b as usize;
            }
            pos += length_size;
            if len == 0 || pos + len > sample.len() {
                return Err(CodecError::Decode(format!("sample NAL of {len} bytes runs past the sample")));
            }
            self.annexb.extend_from_slice(&[0, 0, 0, 1]);
            self.annexb.extend_from_slice(&sample[pos..pos + len]);
            pos += len;
        }

        let deadline = Instant::now() + INPUT_TIMEOUT;
        loop {
            match self.codec.0.dequeue_input_buffer(Duration::from_millis(100)) {
                Ok(DequeuedInputBufferResult::Buffer(mut buffer)) => {
                    let dst = buffer.buffer_mut();
                    if dst.len() < self.annexb.len() {
                        return Err(CodecError::Decode(format!("sample of {} bytes does not fit the input buffer", self.annexb.len())));
                    }
                    for (dst, src) in dst.iter_mut().zip(self.annexb.iter()) {
                        dst.write(*src);
                    }
                    let size = self.annexb.len();
                    // MediaCodec echoes the pts back on the output buffer, so the engine's own pts
                    // round-trips; the unit is microseconds, which we only use as an opaque token.
                    self.codec
                        .0
                        .queue_input_buffer(buffer, 0, size, pts.max(0) as u64, 0)
                        .map_err(|e| CodecError::Decode(format!("queue input: {e}")))?;
                    return Ok(());
                }
                Ok(DequeuedInputBufferResult::TryAgainLater) => {
                    if Instant::now() >= deadline {
                        return Err(CodecError::Decode("no input buffer within 500 ms".into()));
                    }
                }
                Err(e) => return Err(CodecError::Decode(format!("dequeue input: {e}"))),
            }
        }
    }

    /// Drain ready pictures. With `eos` set, keep waiting until the stream ends or the drain
    /// deadline passes (used by `flush`).
    fn drain(&mut self, eos: bool) -> Result<Vec<DecodedFrame>> {
        let mut out = Vec::new();
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        loop {
            match self.codec.0.dequeue_output_buffer(Duration::from_millis(10)) {
                Ok(DequeuedOutputBufferInfoResult::Buffer(buffer)) => {
                    let flags = buffer.info().flags();
                    let pts = buffer.info().presentation_time_us();
                    let end = flags & BUFFER_FLAG_END_OF_STREAM != 0;
                    let size = buffer.info().size().max(0) as usize;
                    // Codec-config and zero-size buffers (the end-of-stream marker) carry no picture.
                    if flags & BUFFER_FLAG_CODEC_CONFIG == 0 && size > 0 {
                        let format = buffer.format();
                        let offset = buffer.info().offset().max(0) as usize;
                        let bytes = buffer.buffer();
                        let data = bytes.get(offset..offset + size).unwrap_or(&[]);
                        let frame = Self::frame_from(&self.info, &mut self.logged_layout, data, &format)?;
                        out.push(DecodedFrame { pts, frame, draft: false });
                    }
                    let _ = self.codec.0.release_output_buffer(buffer, false);
                    if end {
                        return Ok(out);
                    }
                }
                Ok(DequeuedOutputBufferInfoResult::OutputFormatChanged) | Ok(DequeuedOutputBufferInfoResult::OutputBuffersChanged) => {}
                Ok(DequeuedOutputBufferInfoResult::TryAgainLater) => {
                    if !eos || Instant::now() >= deadline {
                        return Ok(out);
                    }
                }
                Err(e) => return Err(CodecError::Decode(format!("dequeue output: {e}"))),
            }
        }
    }

    /// Convert one output buffer into the engine's planar frame, cropping to the display rectangle
    /// and de-interleaving chroma when the device hands back semi-planar (NV12) data.
    fn frame_from(info: &NalStreamInfo, logged_layout: &mut bool, data: &[u8], format: &MediaFormat) -> Result<VideoFrame> {
        let color_format = format.i32("color-format").unwrap_or(COLOR_FORMAT_YUV420_SEMI_PLANAR);
        let (iw, ih) = (info.crop.2 as usize, info.crop.3 as usize);
        let stride = format.i32("stride").filter(|s| *s > 0).map_or(iw, |s| s as usize);
        let vstride = format.i32("slice-height").filter(|s| *s > 0).map_or(ih, |s| s as usize);
        let (ox, oy, w, h) = match format.rect("crop") {
            Some((l, t, r, b)) if r > l && b > t => (l.max(0) as usize, t.max(0) as usize, (r - l) as usize, (b - t) as usize),
            _ => {
                let (cx, cy, cw, ch) = info.crop;
                (cx as usize, cy as usize, cw as usize, ch as usize)
            }
        };
        if !*logged_layout {
            *logged_layout = true;
            log::info!("MediaCodec: output color-format {color_format:#x}, stride {stride}, slice-height {vstride}, picture {w}x{h} at ({ox},{oy})");
        }
        let (cw, chh) = (w.div_ceil(2), h.div_ceil(2));
        let (cox, coy) = (ox / 2, oy / 2);
        let luma_len = stride * vstride;
        if ox + w > stride || oy + h > vstride || data.len() < luma_len {
            return Err(CodecError::Decode(format!("decoded buffer of {} bytes is smaller than {stride}x{vstride}", data.len())));
        }

        let mut y = pool::take_u8(w * h);
        for row in 0..h {
            let start = (oy + row) * stride + ox;
            y.extend_from_slice(&data[start..start + w]);
        }
        let (mut u, mut v) = (pool::take_u8(cw * chh), pool::take_u8(cw * chh));
        let chroma = &data[luma_len.min(data.len())..];
        if color_format == COLOR_FORMAT_YUV420_PLANAR {
            // I420: three tight planes, chroma rows `stride / 2` wide.
            let cstride = (stride / 2).max(cw);
            let plane = cstride * vstride.div_ceil(2);
            for row in 0..chh {
                let start = (coy + row) * cstride + cox;
                if let Some(part) = chroma.get(start..start + cw) {
                    u.extend_from_slice(part);
                }
                if let Some(part) = chroma.get(plane + start..plane + start + cw) {
                    v.extend_from_slice(part);
                }
            }
        } else {
            // NV12/NV21-style interleaved chroma: pairs of (U, V) — take them apart.
            for row in 0..chh {
                let start = (coy + row) * stride + cox * 2;
                let Some(row_bytes) = chroma.get(start..start + cw * 2) else { break };
                for pair in row_bytes.as_chunks::<2>().0 {
                    u.push(pair[0]);
                    v.push(pair[1]);
                }
            }
        }
        if u.len() < cw * chh || v.len() < cw * chh {
            return Err(CodecError::Decode("chroma planes are shorter than the picture".into()));
        }
        Ok(VideoFrame {
            width: w as u32,
            height: h as u32,
            data: PixelData::Yuv8 { planes: [std::sync::Arc::new(y), std::sync::Arc::new(u), std::sync::Arc::new(v)], chroma: Chroma::C420, alpha: None },
            color: info.color,
            par: info.par,
            pts: Tick::ZERO,
        })
    }
}

/// The parameter-set NAL units of an avcC/hvcC record with Annex-B start codes, as MediaCodec's
/// `csd-0` / `csd-1` buffers expect.
fn annexb_nals(nals: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nals.iter().map(Vec::len).sum::<usize>() + 4 * nals.len());
    for nal in nals {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
    }
    out
}

impl VideoDecoder for MediaCodecDecoder {
    fn decode(&mut self, sample: &[u8], pts: i64) -> Result<Vec<DecodedFrame>> {
        self.feed(sample, pts)?;
        self.drain(false)
    }

    fn flush(&mut self) -> Vec<DecodedFrame> {
        // Byte-buffer codecs signal the end of stream by queueing an empty buffer with the EOS
        // flag (the surface API's `signalEndOfInputStream` is not valid here).
        if let Ok(DequeuedInputBufferResult::Buffer(buffer)) = self.codec.0.dequeue_input_buffer(Duration::from_millis(100)) {
            if let Err(e) = self.codec.0.queue_input_buffer(buffer, 0, 0, 0, BUFFER_FLAG_END_OF_STREAM) {
                log::warn!("MediaCodec: cannot signal end of stream: {e}");
            }
        }
        match self.drain(true) {
            Ok(frames) => frames,
            Err(e) => {
                log::warn!("MediaCodec: drain at end of stream failed: {e}");
                Vec::new()
            }
        }
    }

    fn reset(&mut self) {
        // The codec API's `flush` drops pending work and lets decoding restart from a sync sample.
        if let Err(e) = self.codec.0.flush() {
            log::warn!("MediaCodec: flush failed: {e}");
        }
        self.annexb.clear();
    }

    fn name(&self) -> &str {
        "MediaCodec H.264"
    }

    fn is_random_access(&self, sample: &[u8]) -> Option<bool> {
        self.info.is_random_access(sample)
    }

    fn is_disposable(&self, sample: &[u8]) -> bool {
        self.info.is_disposable(sample)
    }
}
