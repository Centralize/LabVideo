//! Reading and decoding media files with Windows' own decoders (Media Foundation).
//!
//! Each open file gets two background threads: one turns the video track into pictures,
//! the other turns the sound track into float samples for the speakers.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use egui::Color32;

/// One decoded picture.
pub struct VideoFrame {
    pub gen: u64,
    pub pts: f64,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Color32>,
}

/// One block of decoded sound, already in the speakers' channel layout.
pub struct AudioChunk {
    pub gen: u64,
    pub pts: f64,
    pub rate: u32,
    pub channels: usize,
    pub data: Vec<f32>,
}

impl AudioChunk {
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels.max(1)
    }
}

/// What we know about a file before playing it.
#[derive(Clone, Debug, Default)]
pub struct MediaInfo {
    pub duration: f64,
    pub has_video: bool,
    pub has_audio: bool,
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: f32,
    pub fps: f64,
    pub video_codec: String,
    pub audio_codec: String,
    pub audio_rate: u32,
    pub audio_channels: u32,
    pub file_size: u64,
}

impl MediaInfo {
    /// Width / height of the picture as it should be shown.
    pub fn display_aspect(&self) -> f32 {
        if self.width == 0 || self.height == 0 {
            return 16.0 / 9.0;
        }
        let par = if self.pixel_aspect > 0.0 { self.pixel_aspect } else { 1.0 };
        self.width as f32 * par / self.height as f32
    }
}

/// Shared between the sound thread and the speakers.
pub struct AudioState {
    pub chunks: VecDeque<AudioChunk>,
    /// Fractional frame position inside the front chunk.
    pub pos: f64,
    /// Generation the speakers accept (changes on every seek).
    pub gen: u64,
    /// Media time of the next sample the speakers will take.
    pub clock: f64,
    /// When `clock` was last updated.
    pub clock_at: Option<std::time::Instant>,
    /// How far ahead of the speakers the samples are handed over.
    pub latency: f64,
    pub playing: bool,
    pub rate: f32,
    pub volume: f32,
    pub muted: bool,
    pub eos: bool,
    pub active: bool,
    /// Peak level of the last block, for the level meter.
    pub level: f32,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            chunks: VecDeque::new(),
            pos: 0.0,
            gen: 0,
            clock: 0.0,
            clock_at: None,
            latency: 0.0,
            playing: false,
            rate: 1.0,
            volume: 1.0,
            muted: false,
            eos: false,
            active: false,
            level: 0.0,
        }
    }
}

impl AudioState {
    pub fn queued_seconds(&self) -> f64 {
        let mut s = 0.0;
        for (i, c) in self.chunks.iter().enumerate() {
            let frames = c.frames() as f64 - if i == 0 { self.pos } else { 0.0 };
            s += frames.max(0.0) / c.rate.max(1) as f64;
        }
        s
    }

    pub fn reset(&mut self, gen: u64, clock: f64) {
        self.chunks.clear();
        self.pos = 0.0;
        self.gen = gen;
        self.clock = clock;
        self.clock_at = None;
        self.eos = false;
    }
}

/// Commands for a decoding thread.
#[derive(Default)]
pub struct DecodeCtl {
    pub quit: AtomicBool,
    pub seek: Mutex<Option<(u64, f64)>>,
    pub eos: AtomicBool,
    pub error: Mutex<Option<String>>,
    pub rate_x1000: AtomicU64,
}

pub struct VideoShared {
    pub ctl: DecodeCtl,
    pub frames: Mutex<VecDeque<VideoFrame>>,
    pub max_frames: AtomicU64,
}

/// A file being decoded.
pub struct Session {
    pub info: MediaInfo,
    pub video: Option<Arc<VideoShared>>,
    pub audio_ctl: Option<Arc<DecodeCtl>>,
    threads: Vec<JoinHandle<()>>,
}

impl Session {
    pub fn seek(&self, gen: u64, t: f64) {
        if let Some(v) = &self.video {
            *v.ctl.seek.lock().unwrap() = Some((gen, t));
            v.ctl.eos.store(false, Ordering::SeqCst);
        }
        if let Some(a) = &self.audio_ctl {
            *a.seek.lock().unwrap() = Some((gen, t));
            a.eos.store(false, Ordering::SeqCst);
        }
    }

    pub fn set_rate(&self, rate: f32) {
        let r = (rate.max(0.001) * 1000.0) as u64;
        if let Some(v) = &self.video {
            v.ctl.rate_x1000.store(r, Ordering::Relaxed);
        }
        if let Some(a) = &self.audio_ctl {
            a.rate_x1000.store(r, Ordering::Relaxed);
        }
    }

    pub fn video_eos(&self) -> bool {
        self.video.as_ref().map(|v| v.ctl.eos.load(Ordering::SeqCst)).unwrap_or(true)
    }

    pub fn error(&self) -> Option<String> {
        if let Some(v) = &self.video {
            if let Some(e) = v.ctl.error.lock().unwrap().clone() {
                return Some(e);
            }
        }
        None
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(v) = &self.video {
            v.ctl.quit.store(true, Ordering::SeqCst);
        }
        if let Some(a) = &self.audio_ctl {
            a.quit.store(true, Ordering::SeqCst);
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Read how long the files are, in the background (like the original's preparsing).
/// Sends (location, seconds) back as each one is read.
pub fn probe_durations(locations: Vec<String>, tx: std::sync::mpsc::Sender<(String, f64)>, repaint: egui::Context) {
    if locations.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new().name("preparse".into()).spawn(move || {
        for loc in locations {
            if loc.contains("://") {
                continue;
            }
            if let Ok(info) = imp::probe(&loc) {
                let p = std::path::Path::new(&loc);
                let raw_ac3 = p.extension().map(|e| e.eq_ignore_ascii_case("ac3")).unwrap_or(false);
                let duration = if raw_ac3 { crate::ac3::raw_file_duration(p).unwrap_or(info.duration) } else { info.duration };
                if tx.send((loc, duration)).is_err() {
                    return;
                }
                repaint.request_repaint();
            }
        }
    });
}

/// Open a file: read what it holds and start decoding at `start` seconds.
pub fn open(
    path: &str,
    gen: u64,
    start: f64,
    audio: Option<(Arc<Mutex<AudioState>>, u32, usize)>,
    repaint: egui::Context,
) -> Result<Session, String> {
    let mut info = imp::probe(path)?;
    info.file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let p = std::path::Path::new(path);
    // a plain .ac3 file is read directly: its frames are all the same size, so times and seeks
    // are exact (the system's reader guesses them from a wrong bit rate)
    let raw_ac3 = p.extension().map(|e| e.eq_ignore_ascii_case("ac3")).unwrap_or(false)
        && crate::ac3::raw_file_duration(p).is_some();
    if raw_ac3 {
        info.duration = crate::ac3::raw_file_duration(p).unwrap_or(info.duration);
        info.has_audio = true;
        info.audio_codec = "Dolby AC-3".into();
    }
    let mut threads = Vec::new();

    let video = if info.has_video {
        let shared = Arc::new(VideoShared {
            ctl: DecodeCtl::default(),
            frames: Mutex::new(VecDeque::new()),
            max_frames: AtomicU64::new(6),
        });
        shared.ctl.rate_x1000.store(1000, Ordering::Relaxed);
        *shared.ctl.seek.lock().unwrap() = Some((gen, start));
        let s2 = shared.clone();
        let p = path.to_string();
        let rp = repaint.clone();
        threads.push(
            std::thread::Builder::new()
                .name("video-decoder".into())
                .spawn(move || imp::video_thread(&p, s2, rp))
                .map_err(|e| e.to_string())?,
        );
        Some(shared)
    } else {
        None
    };

    let audio_ctl = match (info.has_audio, audio) {
        (true, Some((state, dev_rate, dev_channels))) => {
            let ctl = Arc::new(DecodeCtl::default());
            ctl.rate_x1000.store(1000, Ordering::Relaxed);
            *ctl.seek.lock().unwrap() = Some((gen, start));
            let c2 = ctl.clone();
            let p = path.to_string();
            threads.push(
                std::thread::Builder::new()
                    .name("audio-decoder".into())
                    .spawn(move || {
                        if raw_ac3 {
                            raw_ac3_thread(&p, c2, state, dev_channels)
                        } else {
                            imp::audio_thread(&p, c2, state, dev_rate, dev_channels)
                        }
                    })
                    .map_err(|e| e.to_string())?,
            );
            Some(ctl)
        }
        _ => None,
    };

    Ok(Session { info, video, audio_ctl, threads })
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// Hand decoded sound to the speakers' queue: (start time, interleaved samples, rate, channels).
/// After a seek, the part before the target is dropped so the sound starts exactly there.
fn deliver(
    state: &Arc<Mutex<AudioState>>,
    ctl: &DecodeCtl,
    gen: u64,
    skip_until: &mut f64,
    pieces: Vec<(f64, Vec<f32>, u32, usize)>,
    dev_ch: usize,
) {
    for (mut pts, mut floats, rate, ch) in pieces {
        if ch == 0 || rate == 0 {
            continue;
        }
        if *skip_until > 0.0 {
            let frames = floats.len() / ch;
            let end = pts + frames as f64 / rate as f64;
            if end <= *skip_until {
                continue;
            }
            if pts < *skip_until {
                let cut = (((*skip_until - pts) * rate as f64) as usize).min(frames);
                floats.drain(..cut * ch);
                pts = *skip_until;
            }
            *skip_until = -1.0;
        }
        let data = remix(&floats, ch, dev_ch);
        if ctl.seek.lock().unwrap().is_some() {
            break;
        }
        let mut st = state.lock().unwrap();
        if st.gen == gen {
            st.chunks.push_back(AudioChunk { gen, pts, rate, channels: dev_ch, data });
        }
    }
}

/// Plays a plain .ac3 file by reading its frames directly.
fn raw_ac3_thread(path: &str, ctl: Arc<DecodeCtl>, state: Arc<Mutex<AudioState>>, dev_ch: usize) {
    use std::io::{Read, Seek, SeekFrom};
    let finish = |ctl: &DecodeCtl, state: &Arc<Mutex<AudioState>>, gen: u64| {
        ctl.eos.store(true, Ordering::SeqCst);
        let mut st = state.lock().unwrap();
        if st.gen == gen {
            st.eos = true;
        }
    };
    let Ok(mut file) = std::fs::File::open(path) else { return finish(&ctl, &state, 0) };
    let mut head = vec![0u8; 65536];
    let n = file.read(&mut head).unwrap_or(0);
    let Some(sync) = head[..n].windows(2).position(|w| w == [0x0B, 0x77]) else { return finish(&ctl, &state, 0) };
    let (Some(&b4), Some(&b5)) = (head.get(sync + 4), head.get(sync + 5)) else { return finish(&ctl, &state, 0) };
    let fscod = (b4 >> 6) as usize;
    let Some(frame_size) = crate::ac3::frame_bytes(fscod, (b4 & 0x3f) as usize) else { return finish(&ctl, &state, 0) };
    if fscod > 2 {
        return finish(&ctl, &state, 0);
    }
    let rate = crate::ac3::SAMPLE_RATES[fscod] as f64 / match b5 >> 3 { 9 => 2.0, 10 => 4.0, _ => 1.0 };
    let frame_secs = crate::ac3::FRAME_SAMPLES as f64 / rate;

    let mut stream = crate::ac3::Ac3Stream::default();
    let mut gen = 0u64;
    let mut eos = false;
    let mut skip_until = -1.0f64;
    let mut offset = sync as u64; // bytes of the file read so far
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        if ctl.quit.load(Ordering::SeqCst) {
            break;
        }
        if let Some((g, t)) = ctl.seek.lock().unwrap().take() {
            gen = g;
            let index = (t.max(0.0) / frame_secs).floor() as u64;
            offset = sync as u64 + index * frame_size as u64;
            let _ = file.seek(SeekFrom::Start(offset));
            stream.reset();
            skip_until = t;
            eos = false;
            ctl.eos.store(false, Ordering::SeqCst);
        }
        if eos {
            sleep_ms(10);
            continue;
        }
        let speed = ctl.rate_x1000.load(Ordering::Relaxed) as f64 / 1000.0;
        {
            let st = state.lock().unwrap();
            if st.gen == gen && st.queued_seconds() > 1.2 * speed.max(1.0) {
                drop(st);
                sleep_ms(8);
                continue;
            }
        }
        let got = file.read(&mut buf).unwrap_or(0);
        if got == 0 {
            // the last frame has no next frame to confirm it, so decode what's left now
            let pieces = stream.flush().into_iter().map(|(t, f)| (t, f.samples, f.rate, f.channels)).collect();
            deliver(&state, &ctl, gen, &mut skip_until, pieces, dev_ch);
            eos = true;
            finish(&ctl, &state, gen);
            continue;
        }
        // time of the first byte read: whole frames so far (a partial frame keeps its running time)
        let pts = (offset - sync as u64) as f64 / frame_size as f64 * frame_secs;
        offset += got as u64;
        let pieces = stream
            .push(&buf[..got], pts)
            .into_iter()
            .map(|(t, f)| (t, f.samples, f.rate, f.channels))
            .collect();
        deliver(&state, &ctl, gen, &mut skip_until, pieces, dev_ch);
    }
}

/// Turn a 5.1 / mono / other layout into the speakers' layout.
fn remix(src: &[f32], src_ch: usize, dst_ch: usize) -> Vec<f32> {
    if src_ch == dst_ch || src_ch == 0 {
        return src.to_vec();
    }
    let frames = src.len() / src_ch;
    let mut out = Vec::with_capacity(frames * dst_ch);
    for f in 0..frames {
        let s = &src[f * src_ch..(f + 1) * src_ch];
        if dst_ch == 2 && src_ch == 1 {
            out.push(s[0]);
            out.push(s[0]);
        } else if dst_ch == 2 && src_ch >= 6 {
            // FL FR C LFE SL SR → stereo like the original's channel mixer
            // (libsimple_channel_mixer FUN_140001770): L + k·(C + SL), R + k·(C + SR), LFE dropped
            let k = crate::mechanics::SURROUND_MIX;
            out.push(s[0] + k * (s[2] + s[4]));
            out.push(s[1] + k * (s[2] + s[5]));
        } else if dst_ch == 1 {
            let sum: f32 = s.iter().sum();
            out.push(sum / src_ch as f32);
        } else {
            for c in 0..dst_ch {
                out.push(if c < src_ch { s[c] } else { s[c % src_ch] });
            }
        }
    }
    out
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::core::{Interface, GUID, PCWSTR};
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    use windows::Win32::System::Variant::VT_I8;

    const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    const ALL: u32 = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
    const SOURCE: u32 = MF_SOURCE_READER_MEDIASOURCE.0 as u32;

    struct MfScope;
    impl MfScope {
        fn new() -> Result<Self, String> {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).map_err(|e| e.message().to_string())?;
            }
            Ok(MfScope)
        }
    }
    impl Drop for MfScope {
        fn drop(&mut self) {
            unsafe {
                let _ = MFShutdown();
                CoUninitialize();
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn reader(path: &str, processing: bool) -> windows::core::Result<IMFSourceReader> {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2)?;
        let attrs = attrs.unwrap();
        if processing {
            attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
        }
        let w = wide(path);
        MFCreateSourceReaderFromURL(PCWSTR(w.as_ptr()), &attrs)
    }

    fn fourcc_name(g: &GUID) -> String {
        let known: &[(GUID, &str)] = &[
            (MFVideoFormat_H264, "H.264 / AVC"),
            (MFVideoFormat_HEVC, "H.265 / HEVC"),
            (MFVideoFormat_MP4V, "MPEG-4 Part 2"),
            (MFVideoFormat_MPEG2, "MPEG-2 Video"),
            (MFVideoFormat_WMV3, "Windows Media Video 9"),
            (MFVideoFormat_WVC1, "VC-1"),
            (MFVideoFormat_VP90, "VP9"),
            (MFVideoFormat_AV1, "AV1"),
            (MFVideoFormat_MJPG, "Motion JPEG"),
            (MFAudioFormat_AAC, "AAC"),
            (MFAudioFormat_MP3, "MPEG Audio Layer 3"),
            (MFAudioFormat_MPEG, "MPEG Audio"),
            (MFAudioFormat_PCM, "PCM"),
            (MFAudioFormat_Float, "PCM float"),
            (MFAudioFormat_FLAC, "FLAC"),
            (MFAudioFormat_ALAC, "ALAC"),
            (MFAudioFormat_Opus, "Opus"),
            (MFAudioFormat_Dolby_AC3, "Dolby AC-3"),
            (MFAudioFormat_Dolby_DDPlus, "Dolby E-AC-3"),
            (MFAudioFormat_WMAudioV8, "Windows Media Audio"),
            (MFAudioFormat_WMAudioV9, "Windows Media Audio 9"),
        ];
        for (k, n) in known {
            if k == g {
                return n.to_string();
            }
        }
        let b = g.data1.to_le_bytes();
        if b.iter().all(|c| c.is_ascii_graphic() || *c == b' ') {
            String::from_utf8_lossy(&b).trim().to_string()
        } else {
            format!("0x{:x}", g.data1)
        }
    }

    pub fn probe(path: &str) -> Result<MediaInfo, String> {
        let _mf = MfScope::new()?;
        unsafe {
            let r = reader(path, false).map_err(|e| e.message().to_string())?;
            let mut info = MediaInfo { pixel_aspect: 1.0, ..Default::default() };
            if let Ok(pv) = r.GetPresentationAttribute(SOURCE, &MF_PD_DURATION) {
                let d = pv.Anonymous.Anonymous.Anonymous.uhVal;
                info.duration = d as f64 / 1e7;
            }
            if let Ok(mt) = r.GetNativeMediaType(VIDEO, 0) {
                info.has_video = true;
                if let Ok(st) = mt.GetGUID(&MF_MT_SUBTYPE) {
                    info.video_codec = fourcc_name(&st);
                }
                if let Ok(fs) = mt.GetUINT64(&MF_MT_FRAME_SIZE) {
                    info.width = (fs >> 32) as u32;
                    info.height = fs as u32;
                }
                if let Ok(fr) = mt.GetUINT64(&MF_MT_FRAME_RATE) {
                    let (n, d) = ((fr >> 32) as u32, fr as u32);
                    if d > 0 {
                        info.fps = n as f64 / d as f64;
                    }
                }
                if let Ok(par) = mt.GetUINT64(&MF_MT_PIXEL_ASPECT_RATIO) {
                    let (n, d) = ((par >> 32) as u32, par as u32);
                    if n > 0 && d > 0 {
                        info.pixel_aspect = n as f32 / d as f32;
                    }
                }
            }
            if let Ok(mt) = r.GetNativeMediaType(AUDIO, 0) {
                info.has_audio = true;
                if let Ok(st) = mt.GetGUID(&MF_MT_SUBTYPE) {
                    info.audio_codec = fourcc_name(&st);
                }
                info.audio_rate = mt.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(0);
                info.audio_channels = mt.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(0);
            }
            if !info.has_video && !info.has_audio {
                return Err("This file has no sound or picture this player can read.".into());
            }
            Ok(info)
        }
    }

    unsafe fn set_position(r: &IMFSourceReader, t: f64) {
        let mut pv = PROPVARIANT::default();
        (*pv.Anonymous.Anonymous).vt = VT_I8;
        (*pv.Anonymous.Anonymous).Anonymous.hVal = (t.max(0.0) * 1e7) as i64;
        let _ = r.SetCurrentPosition(&GUID::zeroed(), &pv);
    }

    struct VideoFormat {
        #[allow(dead_code)]
        width: usize,
        height: usize,
        stride: i32,
        crop: (usize, usize, usize, usize),
    }

    unsafe fn video_format(r: &IMFSourceReader) -> windows::core::Result<VideoFormat> {
        let mt = r.GetCurrentMediaType(VIDEO)?;
        let fs = mt.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let width = (fs >> 32) as usize;
        let height = (fs as u32) as usize;
        let stride = mt.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or((width * 4) as i32);
        let mut crop = (0, 0, width, height);
        let mut blob = [0u8; 16];
        if mt.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, &mut blob, None).is_ok() {
            let ox = i16::from_le_bytes([blob[2], blob[3]]).max(0) as usize;
            let oy = i16::from_le_bytes([blob[6], blob[7]]).max(0) as usize;
            let cx = i32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]).max(0) as usize;
            let cy = i32::from_le_bytes([blob[12], blob[13], blob[14], blob[15]]).max(0) as usize;
            if cx > 0 && cy > 0 && ox + cx <= width && oy + cy <= height {
                crop = (ox, oy, cx, cy);
            }
        }
        Ok(VideoFormat { width, height, stride, crop })
    }

    /// Copy the visible part of a BGRX picture. `top` is the first row, row y is at `top + y*pitch`.
    unsafe fn copy_rows(top: *const u8, pitch: isize, fmt: &VideoFormat, lo: usize, hi: usize) -> Vec<Color32> {
        let (ox, oy, cw, ch) = fmt.crop;
        let mut out = Vec::with_capacity(cw * ch);
        for y in oy..oy + ch {
            let rp = top.offset(y as isize * pitch + (ox * 4) as isize);
            let a = rp as usize;
            if a < lo || a + cw * 4 > hi {
                out.extend(std::iter::repeat(Color32::BLACK).take(cw));
                continue;
            }
            let row = std::slice::from_raw_parts(rp, cw * 4);
            out.extend(row.chunks_exact(4).map(|px| Color32::from_rgb(px[2], px[1], px[0])));
        }
        out
    }

    unsafe fn convert(sample: &IMFSample, fmt: &VideoFormat) -> windows::core::Result<(usize, usize, Vec<Color32>)> {
        let buf = sample.ConvertToContiguousBuffer()?;
        let (_, _, cw, ch) = fmt.crop;
        let out;
        if let Ok(b2) = buf.cast::<IMF2DBuffer>() {
            let mut p: *mut u8 = std::ptr::null_mut();
            let mut pitch: i32 = 0;
            b2.Lock2D(&mut p, &mut pitch)?;
            // Lock2D hands back the top row; a negative pitch means rows go upwards in memory.
            let span = pitch.unsigned_abs() as usize * fmt.height;
            let (lo, hi) = if pitch >= 0 {
                (p as usize, p as usize + span)
            } else {
                let bottom = p as usize - (fmt.height - 1) * pitch.unsigned_abs() as usize;
                (bottom, bottom + span)
            };
            out = copy_rows(p, pitch as isize, fmt, lo, hi);
            let _ = b2.Unlock2D();
        } else {
            let mut p: *mut u8 = std::ptr::null_mut();
            let mut len: u32 = 0;
            buf.Lock(&mut p, None, Some(&mut len))?;
            let stride = fmt.stride as isize;
            let (lo, hi) = (p as usize, p as usize + len as usize);
            out = if stride >= 0 {
                copy_rows(p, stride, fmt, lo, hi)
            } else {
                let top = p.offset((fmt.height as isize - 1) * -stride);
                copy_rows(top, stride, fmt, lo, hi)
            };
            let _ = buf.Unlock();
        }
        let mut out = out;
        if out.len() != cw * ch {
            out.resize(cw * ch, Color32::BLACK);
        }
        Ok((cw, ch, out))
    }

    pub fn video_thread(path: &str, shared: Arc<VideoShared>, repaint: egui::Context) {
        let fail = |msg: String| {
            *shared.ctl.error.lock().unwrap() = Some(msg);
            shared.ctl.eos.store(true, Ordering::SeqCst);
        };
        let _mf = match MfScope::new() {
            Ok(m) => m,
            Err(e) => return fail(e),
        };
        unsafe {
            let r = match reader(path, true) {
                Ok(r) => r,
                Err(e) => return fail(e.message().to_string()),
            };
            let _ = r.SetStreamSelection(ALL, false);
            let _ = r.SetStreamSelection(VIDEO, true);
            let setup = (|| -> windows::core::Result<()> {
                let mt = MFCreateMediaType()?;
                mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
                r.SetCurrentMediaType(VIDEO, None, &mt)
            })();
            if let Err(e) = setup {
                return fail(format!("The picture can't be decoded: {}", e.message()));
            }
            let mut fmt = match video_format(&r) {
                Ok(f) => f,
                Err(e) => return fail(e.message().to_string()),
            };
            let frame_bytes = (fmt.crop.2 * fmt.crop.3 * 4).max(1);
            let max = (128 * 1024 * 1024 / frame_bytes).clamp(3, 8);
            shared.max_frames.store(max as u64, Ordering::Relaxed);

            let mut gen = 0u64;
            let mut skip_until = -1.0f64;
            let mut eos = false;
            loop {
                if shared.ctl.quit.load(Ordering::SeqCst) {
                    break;
                }
                if let Some((g, t)) = shared.ctl.seek.lock().unwrap().take() {
                    gen = g;
                    set_position(&r, t);
                    skip_until = t - 0.02;
                    eos = false;
                    shared.ctl.eos.store(false, Ordering::SeqCst);
                    shared.frames.lock().unwrap().retain(|f| f.gen == gen);
                }
                if eos {
                    sleep_ms(10);
                    continue;
                }
                let queued = shared.frames.lock().unwrap().iter().filter(|f| f.gen == gen).count();
                if queued >= shared.max_frames.load(Ordering::Relaxed) as usize {
                    sleep_ms(3);
                    continue;
                }
                let mut flags = 0u32;
                let mut ts = 0i64;
                let mut sample: Option<IMFSample> = None;
                if let Err(e) = r.ReadSample(VIDEO, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample)) {
                    fail(e.message().to_string());
                    eos = true;
                    continue;
                }
                if flags & (MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32) != 0 {
                    if let Ok(f) = video_format(&r) {
                        fmt = f;
                    }
                }
                if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 || flags & (MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
                    eos = true;
                    shared.ctl.eos.store(true, Ordering::SeqCst);
                    repaint.request_repaint();
                    continue;
                }
                let Some(sample) = sample else { continue };
                let pts = sample.GetSampleTime().unwrap_or(ts) as f64 / 1e7;
                if pts < skip_until {
                    continue;
                }
                skip_until = -1.0;
                if let Ok((w, h, pixels)) = convert(&sample, &fmt) {
                    // a newer seek may have arrived while decoding
                    if shared.ctl.seek.lock().unwrap().is_some() {
                        continue;
                    }
                    shared.frames.lock().unwrap().push_back(VideoFrame { gen, pts, width: w, height: h, pixels });
                    repaint.request_repaint();
                }
            }
        }
    }

    pub fn audio_thread(path: &str, ctl: Arc<DecodeCtl>, state: Arc<Mutex<AudioState>>, _dev_rate: u32, dev_ch: usize) {
        let _mf = match MfScope::new() {
            Ok(m) => m,
            Err(_) => {
                ctl.eos.store(true, Ordering::SeqCst);
                return;
            }
        };
        unsafe {
            let Ok(r) = reader(path, false) else {
                ctl.eos.store(true, Ordering::SeqCst);
                state.lock().unwrap().eos = true;
                return;
            };
            let _ = r.SetStreamSelection(ALL, false);
            let _ = r.SetStreamSelection(AUDIO, true);
            let ok = (|| -> windows::core::Result<(u32, usize)> {
                let mt = MFCreateMediaType()?;
                mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                mt.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)?;
                r.SetCurrentMediaType(AUDIO, None, &mt)?;
                let cur = r.GetCurrentMediaType(AUDIO)?;
                let rate = cur.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
                let ch = cur.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)? as usize;
                Ok((rate, ch))
            })();
            // Windows can't decode it: Dolby AC-3 is decoded by the player itself, from the
            // compressed frames the file holds (like the original does with its own decoder)
            let mut ac3: Option<crate::ac3::Ac3Stream> = None;
            let (src_rate, src_ch) = match ok {
                Ok(v) => v,
                Err(_) => {
                    let native = r.GetNativeMediaType(AUDIO, 0);
                    let is_ac3 = native
                        .as_ref()
                        .ok()
                        .and_then(|t| t.GetGUID(&MF_MT_SUBTYPE).ok())
                        .map(|g| g == MFAudioFormat_Dolby_AC3)
                        .unwrap_or(false);
                    match native {
                        Ok(t) if is_ac3 && r.SetCurrentMediaType(AUDIO, None, &t).is_ok() => {
                            ac3 = Some(crate::ac3::Ac3Stream::default());
                            (0, 0)
                        }
                        _ => {
                            ctl.eos.store(true, Ordering::SeqCst);
                            state.lock().unwrap().eos = true;
                            return;
                        }
                    }
                }
            };
            let mut gen = 0u64;
            let mut eos = false;
            let mut skip_until = -1.0f64;
            loop {
                if ctl.quit.load(Ordering::SeqCst) {
                    break;
                }
                if let Some((g, t)) = ctl.seek.lock().unwrap().take() {
                    gen = g;
                    set_position(&r, t);
                    eos = false;
                    skip_until = t;
                    ctl.eos.store(false, Ordering::SeqCst);
                    if let Some(a) = &mut ac3 {
                        a.reset();
                    }
                }
                if eos {
                    sleep_ms(10);
                    continue;
                }
                let rate = ctl.rate_x1000.load(Ordering::Relaxed) as f64 / 1000.0;
                let ahead = 1.2 * rate.max(1.0);
                {
                    let st = state.lock().unwrap();
                    if st.gen == gen && st.queued_seconds() > ahead {
                        drop(st);
                        sleep_ms(8);
                        continue;
                    }
                }
                let mut flags = 0u32;
                let mut ts = 0i64;
                let mut sample: Option<IMFSample> = None;
                if r.ReadSample(AUDIO, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample)).is_err() {
                    eos = true;
                }
                if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 || flags & (MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
                    eos = true;
                }
                if eos {
                    if let Some(a) = &mut ac3 {
                        // the last frame has no next frame to confirm it, so decode what's left now
                        let pieces = a.flush().into_iter().map(|(t, f)| (t, f.samples, f.rate, f.channels)).collect();
                        deliver(&state, &ctl, gen, &mut skip_until, pieces, dev_ch);
                    }
                    ctl.eos.store(true, Ordering::SeqCst);
                    let mut st = state.lock().unwrap();
                    if st.gen == gen {
                        st.eos = true;
                    }
                    continue;
                }
                let Some(sample) = sample else { continue };
                let sample_pts = sample.GetSampleTime().unwrap_or(ts) as f64 / 1e7;
                let Ok(buf) = sample.ConvertToContiguousBuffer() else { continue };
                let mut p: *mut u8 = std::ptr::null_mut();
                let mut len: u32 = 0;
                if buf.Lock(&mut p, None, Some(&mut len)).is_err() {
                    continue;
                }
                // decoded pieces: (start time, interleaved samples, rate, channels)
                let mut pieces: Vec<(f64, Vec<f32>, u32, usize)> = Vec::new();
                match &mut ac3 {
                    None => {
                        let n = len as usize / 4;
                        let mut floats = vec![0f32; n];
                        std::ptr::copy_nonoverlapping(p as *const f32, floats.as_mut_ptr(), n);
                        pieces.push((sample_pts, floats, src_rate, src_ch));
                    }
                    Some(a) => {
                        let bytes = std::slice::from_raw_parts(p as *const u8, len as usize);
                        for (pts, f) in a.push(bytes, sample_pts) {
                            pieces.push((pts, f.samples, f.rate, f.channels));
                        }
                    }
                }
                let _ = buf.Unlock();
                deliver(&state, &ctl, gen, &mut skip_until, pieces, dev_ch);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub fn probe(_path: &str) -> Result<MediaInfo, String> {
        Err("Playing files needs Windows in this version.".into())
    }
    pub fn video_thread(_path: &str, _shared: Arc<VideoShared>, _repaint: egui::Context) {}
    pub fn audio_thread(_path: &str, _ctl: Arc<DecodeCtl>, _state: Arc<Mutex<AudioState>>, _r: u32, _c: usize) {}
}
