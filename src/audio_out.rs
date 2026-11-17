//! Sending decoded sound to the speakers. The speakers also keep the playback clock.
//!
//! At speeds other than 1x the sound keeps its pitch: it is cut into short overlapping
//! pieces that are laid down at the new pace and cross-faded where they meet, with each piece
//! nudged to where it lines up best with the previous one (stride 30 ms, overlap 20 %,
//! search 14 ms, as in the original's tempo scaler settings).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::mechanics::{AUDIBLE_RATE_MAX, AUDIBLE_RATE_MIN, TEMPO_OVERLAP, TEMPO_SEARCH_MS, TEMPO_STRIDE_MS};
use crate::media::AudioState;

pub struct AudioOut {
    pub state: Arc<Mutex<AudioState>>,
    pub rate: u32,
    pub channels: usize,
    _stream: cpal::Stream,
}

impl AudioOut {
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let config = supported.config();
        let state = Arc::new(Mutex::new(AudioState::default()));
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, config, state.clone(), rate, channels),
            cpal::SampleFormat::I16 => build::<i16>(&device, config, state.clone(), rate, channels),
            cpal::SampleFormat::U16 => build::<u16>(&device, config, state.clone(), rate, channels),
            cpal::SampleFormat::I32 => build::<i32>(&device, config, state.clone(), rate, channels),
            _ => build::<f32>(&device, config, state.clone(), rate, channels),
        }?;
        stream.play().ok()?;
        Some(Self { state, rate, channels, _stream: stream })
    }
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    state: Arc<Mutex<AudioState>>,
    dev_rate: u32,
    channels: usize,
) -> Option<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let mut scratch: Vec<f32> = Vec::new();
    let mut stretch = Stretch::new(dev_rate, channels);
    device
        .build_output_stream(
            config,
            move |out: &mut [T], info: &cpal::OutputCallbackInfo| {
                scratch.clear();
                scratch.resize(out.len(), 0.0);
                let ts = info.timestamp();
                // the first sample of this block is heard after `latency`; the clock marks its end
                let block = (out.len() / channels.max(1)) as f64 / dev_rate.max(1) as f64;
                let latency = ts.playback.duration_since(ts.callback).as_secs_f64() + block;
                if let Ok(mut st) = state.lock() {
                    st.latency = latency;
                    fill(&mut st, &mut stretch, &mut scratch, dev_rate, channels);
                }
                for (o, s) in out.iter_mut().zip(scratch.iter()) {
                    *o = T::from_sample(*s);
                }
            },
            |_err| {},
            None,
        )
        .ok()
}

/// Media time of the next unread sample in the queue.
fn next_pts(st: &AudioState) -> Option<f64> {
    st.chunks.front().map(|c| c.pts + st.pos / c.rate.max(1) as f64)
}

/// Read up to `frames` frames from the queue, `speed` times faster than real time, resampled to
/// the device rate. Returns how many frames were written into `out`.
fn read_frames(st: &mut AudioState, out: &mut [f32], frames: usize, channels: usize, dev_rate: u32, speed: f64) -> usize {
    let gen = st.gen;
    while st.chunks.front().map(|c| c.gen != gen).unwrap_or(false) {
        st.chunks.pop_front();
        st.pos = 0.0;
    }
    let mut f = 0;
    while f < frames {
        let Some(chunk) = st.chunks.front() else { break };
        let n = chunk.frames();
        if n == 0 || chunk.channels != channels {
            st.chunks.pop_front();
            st.pos = 0.0;
            continue;
        }
        let i = st.pos.floor() as usize;
        let frac = (st.pos - i as f64) as f32;
        let i0 = i.min(n - 1);
        let i1 = (i + 1).min(n - 1);
        for c in 0..channels {
            let a = chunk.data[i0 * channels + c];
            let b = chunk.data[i1 * channels + c];
            out[f * channels + c] = a + (b - a) * frac;
        }
        st.pos += chunk.rate as f64 / dev_rate as f64 * speed;
        if st.pos >= n as f64 {
            st.pos -= n as f64;
            st.chunks.pop_front();
        }
        f += 1;
    }
    f
}

/// Pitch-keeping speed change.
struct Stretch {
    rate: u32,
    channels: usize,
    gen: u64,
    active: bool,
    /// Input at 1x, device rate, interleaved.
    input: Vec<f32>,
    /// Media time of `input[0]`.
    input_pts: f64,
    /// Where the next piece starts in `input` (frames, fractional).
    pos: f64,
    /// End of the previous piece, to cross-fade with.
    tail: Option<Vec<f32>>,
    /// Finished output waiting to be played.
    out: Vec<f32>,
    out_read: usize,
    /// Media time of the first frame of the piece being played, and its pace.
    out_pts: f64,
    out_frames_done: usize,
    scratch: Vec<f32>,
}

impl Stretch {
    fn new(rate: u32, channels: usize) -> Self {
        Self {
            rate,
            channels,
            gen: 0,
            active: false,
            input: Vec::new(),
            input_pts: 0.0,
            pos: 0.0,
            tail: None,
            out: Vec::new(),
            out_read: 0,
            out_pts: 0.0,
            out_frames_done: 0,
            scratch: Vec::new(),
        }
    }

    fn reset(&mut self) {
        self.active = false;
        self.input.clear();
        self.pos = 0.0;
        self.tail = None;
        self.out.clear();
        self.out_read = 0;
        self.out_frames_done = 0;
    }

    fn stride(&self) -> usize {
        ((TEMPO_STRIDE_MS / 1000.0) * self.rate as f64).round().max(64.0) as usize
    }
    fn overlap(&self) -> usize {
        ((self.stride() as f64) * TEMPO_OVERLAP).round().max(8.0) as usize
    }
    fn search(&self) -> usize {
        ((TEMPO_SEARCH_MS / 1000.0) * self.rate as f64).round() as usize
    }

    fn input_frames(&self) -> usize {
        self.input.len() / self.channels
    }

    /// Make one more piece of output. Returns false when the queue ran dry.
    fn make_piece(&mut self, st: &mut AudioState, speed: f64) -> bool {
        let ch = self.channels;
        let (stride, overlap, search) = (self.stride(), self.overlap(), self.search());
        let start = self.pos.floor() as usize;
        let need = start + search + stride + overlap;
        // top up the input
        while self.input_frames() < need {
            let want = (need - self.input_frames()).max(1024);
            if self.input.is_empty() {
                match next_pts(st) {
                    Some(p) => self.input_pts = p,
                    None => return false,
                }
            }
            self.scratch.clear();
            self.scratch.resize(want * ch, 0.0);
            let got = read_frames(st, &mut self.scratch, want, ch, self.rate, 1.0);
            if got == 0 {
                return false;
            }
            self.input.extend_from_slice(&self.scratch[..got * ch]);
        }
        // find where the new piece lines up best with the previous tail
        let best = match &self.tail {
            None => 0,
            Some(tail) => {
                let mut best_k = 0;
                let mut best_score = f32::MIN;
                for k in 0..search {
                    let base = (start + k) * ch;
                    let mut dot = 0.0f32;
                    let mut energy = 1e-9f32;
                    for i in 0..overlap {
                        let mut a = 0.0;
                        let mut b = 0.0;
                        for c in 0..ch {
                            a += tail[i * ch + c];
                            b += self.input[base + i * ch + c];
                        }
                        // weight the middle of the overlap more
                        let w = (i * (overlap - i)) as f32;
                        dot += a * b * w;
                        energy += b * b * w;
                    }
                    let score = dot / energy.sqrt();
                    if score > best_score {
                        best_score = score;
                        best_k = k;
                    }
                }
                best_k
            }
        };
        let seg = (start + best) * ch;
        // output: cross-fade the overlap, then the rest of the stride
        self.out.drain(..self.out_read * ch);
        self.out_read = 0;
        match &self.tail {
            Some(tail) => {
                for i in 0..overlap {
                    let t = (i as f32 + 0.5) / overlap as f32;
                    for c in 0..ch {
                        let a = tail[i * ch + c];
                        let b = self.input[seg + i * ch + c];
                        self.out.push(a * (1.0 - t) + b * t);
                    }
                }
            }
            None => self.out.extend_from_slice(&self.input[seg..seg + overlap * ch]),
        }
        self.out.extend_from_slice(&self.input[seg + overlap * ch..seg + stride * ch]);
        self.tail = Some(self.input[seg + stride * ch..seg + (stride + overlap) * ch].to_vec());
        self.out_pts = self.input_pts + (start + best) as f64 / self.rate as f64;
        self.out_frames_done = 0;
        // move on by stride × speed in the input, and forget what's behind
        self.pos += stride as f64 * speed;
        let drop = (self.pos.floor() as usize).min(self.input_frames());
        if drop > 0 {
            self.input.drain(..drop * ch);
            self.input_pts += drop as f64 / self.rate as f64;
            self.pos -= drop as f64;
        }
        true
    }
}

fn fill(st: &mut AudioState, stretch: &mut Stretch, out: &mut [f32], dev_rate: u32, channels: usize) {
    if st.gen != stretch.gen {
        stretch.gen = st.gen;
        stretch.reset();
    }
    if !st.active || !st.playing || channels == 0 {
        st.level = 0.0;
        return;
    }
    let speed = st.rate as f64;
    let audible = st.rate >= AUDIBLE_RATE_MIN && st.rate <= AUDIBLE_RATE_MAX;
    let gain = if st.muted { 0.0 } else { st.volume };
    let frames = out.len() / channels;
    let mut peak = 0f32;

    if !audible || (speed - 1.0).abs() < 1e-3 || !crate::mechanics::KEEP_PITCH {
        // plain path: no pitch keeping needed (or the sound is silent at this speed)
        if stretch.active {
            stretch.reset();
        }
        let got = read_frames(st, out, frames, channels, dev_rate, speed);
        if let Some(p) = next_pts(st) {
            st.clock = p;
        } else if got > 0 {
            st.clock += got as f64 * speed / dev_rate as f64;
        }
        for v in out[..got * channels].iter_mut() {
            *v = if audible { (*v * gain).clamp(-1.0, 1.0) } else { 0.0 };
            peak = peak.max(v.abs());
        }
    } else {
        if !stretch.active {
            stretch.reset();
            stretch.active = true;
        }
        let mut f = 0;
        while f < frames {
            let avail = stretch.out.len() / channels - stretch.out_read;
            if avail == 0 {
                if !stretch.make_piece(st, speed) {
                    break;
                }
                continue;
            }
            let n = avail.min(frames - f);
            let from = stretch.out_read * channels;
            for i in 0..n * channels {
                let v = (stretch.out[from + i] * gain).clamp(-1.0, 1.0);
                out[f * channels + i] = v;
                peak = peak.max(v.abs());
            }
            stretch.out_read += n;
            stretch.out_frames_done += n;
            f += n;
            st.clock = stretch.out_pts + stretch.out_frames_done as f64 * speed / dev_rate as f64;
        }
    }
    st.clock_at = Some(Instant::now());
    st.level = peak;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::AudioChunk;

    fn tone_state(rate: u32, secs: f64, speed: f32) -> AudioState {
        let mut st = AudioState::default();
        st.active = true;
        st.playing = true;
        st.rate = speed;
        let n = (rate as f64 * secs) as usize;
        let mut data = Vec::with_capacity(n * 2);
        for i in 0..n {
            let v = (i as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            data.push(v);
            data.push(v);
        }
        st.chunks.push_back(AudioChunk { gen: 0, pts: 0.0, rate, channels: 2, data });
        st
    }

    fn crossings(samples: &[f32], ch: usize) -> usize {
        let mut c = 0;
        for i in 1..samples.len() / ch {
            if (samples[(i - 1) * ch] < 0.0) != (samples[i * ch] < 0.0) {
                c += 1;
            }
        }
        c
    }

    #[test]
    fn keeps_pitch_and_pace() {
        for speed in [1.5f32, 0.75, 2.0] {
            let rate = 48000;
            let mut st = tone_state(rate, 6.0, speed);
            let mut s = Stretch::new(rate, 2);
            let mut out = vec![0.0f32; 1024 * 2];
            let mut all = Vec::new();
            for _ in 0..60 {
                fill(&mut st, &mut s, &mut out, rate, 2);
                all.extend_from_slice(&out);
            }
            let secs = all.len() as f64 / 2.0 / rate as f64;
            let hz = crossings(&all, 2) as f64 / 2.0 / secs;
            assert!((hz - 440.0).abs() < 20.0, "speed {speed}: heard {hz} Hz");
            // the clock moved by about speed × played time
            let expect = secs * speed as f64;
            assert!((st.clock - expect).abs() < 0.1, "speed {speed}: clock {} expected {}", st.clock, expect);
        }
    }
}

/// Current media time heard from the speakers, smoothed between callbacks.
pub fn heard_clock(st: &AudioState) -> Option<f64> {
    let at = st.clock_at?;
    let mut t = st.clock - st.latency * st.rate as f64;
    if st.playing && !st.chunks.is_empty() {
        let dt = at.elapsed().as_secs_f64().min(0.05);
        t += dt * st.rate as f64;
    }
    Some(t.max(0.0))
}
