//! The player: playlist, play/pause/stop, seeking, speed, volume, and the clock.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use egui::{ColorImage, TextureHandle, TextureOptions};

use crate::audio_out::{heard_clock, AudioOut};
use crate::mechanics;
use crate::media::{self, MediaInfo, Session};
use crate::subtitles::Subtitles;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoopMode {
    Off,
    All,
    One,
}

impl LoopMode {
    pub fn next(self) -> Self {
        match self {
            LoopMode::Off => LoopMode::All,
            LoopMode::All => LoopMode::One,
            LoopMode::One => LoopMode::Off,
        }
    }
}

/// Icons the original draws on the video (OSD_PLAY_ICON = 1, OSD_PAUSE_ICON = 2, OSD_MUTE_ICON = 4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OsdIcon {
    Play,
    Pause,
    Mute,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub location: String,
    pub title: String,
    pub duration: Option<f64>,
}

impl Item {
    pub fn new(location: &str) -> Self {
        let title = title_of(location);
        Self { location: location.to_string(), title, duration: None }
    }
}

pub fn title_of(location: &str) -> String {
    if location.contains("://") {
        return location.rsplit('/').find(|s| !s.is_empty()).unwrap_or(location).to_string();
    }
    Path::new(location)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| location.to_string())
}

pub const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "asf", "mpg", "mpeg", "ts", "m2ts", "3gp", "flv",
    "mp3", "m4a", "aac", "wav", "flac", "wma", "ogg", "opus", "aiff", "aif", "ac3", "mka",
];

pub fn is_media(path: &Path) -> bool {
    path.extension()
        .map(|e| MEDIA_EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
        .unwrap_or(false)
}

pub struct Player {
    pub playlist: Vec<Item>,
    pub current: Option<usize>,
    pub state: State,
    pub session: Option<Session>,
    pub info: Option<MediaInfo>,
    pub gen: u64,
    pub rate: f32,
    pub volume: f32,
    pub muted: bool,
    pub loop_mode: LoopMode,
    pub random: bool,
    pub quit_at_end: bool,
    pub want_quit: bool,
    pub error: Option<String>,
    pub osd: Option<(String, Instant)>,
    pub texture: Option<TextureHandle>,
    pub frame: Option<Arc<ColorImage>>,
    pub frame_pts: f64,
    frame_dirty: bool,
    pub aspect_index: usize,
    pub subtitles: Option<Subtitles>,
    pub subtitles_on: bool,
    /// Subtitle text size in percent (100 = normal).
    pub sub_scale: i32,
    pub recent: Vec<String>,
    audio: Option<AudioOut>,
    sys_base: f64,
    sys_at: Instant,
    buffering: bool,
    need_preview: bool,
    stepped: bool,
    rng: u64,
    pub jump_sizes: [f64; 4],
    /// A→B loop points.
    pub ab: (Option<f64>, Option<f64>),
    /// Play order (shuffled when Random is on) and where we are in it.
    order: Vec<usize>,
    order_pos: Option<usize>,
    order_dirty: bool,
    /// A jump went past the end: finish the item at the next update.
    end_requested: bool,
    /// On-screen icon (play, pause, mute), shown like a message.
    pub osd_icon: Option<(OsdIcon, Instant)>,
    to_probe: Vec<String>,
    durations_tx: std::sync::mpsc::Sender<(String, f64)>,
    durations_rx: std::sync::mpsc::Receiver<(String, f64)>,
}

impl Player {
    pub fn new(with_audio: bool) -> Self {
        let audio = if with_audio { AudioOut::new() } else { None };
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15)
            | 1;
        let (durations_tx, durations_rx) = std::sync::mpsc::channel();
        Self {
            ab: (None, None),
            order: Vec::new(),
            order_pos: None,
            order_dirty: true,
            end_requested: false,
            osd_icon: None,
            to_probe: Vec::new(),
            durations_tx,
            durations_rx,
            playlist: Vec::new(),
            current: None,
            state: State::Stopped,
            session: None,
            info: None,
            gen: 1,
            rate: 1.0,
            volume: 1.0,
            muted: false,
            loop_mode: LoopMode::Off,
            random: false,
            quit_at_end: false,
            want_quit: false,
            error: None,
            osd: None,
            texture: None,
            frame: None,
            frame_pts: 0.0,
            frame_dirty: false,
            aspect_index: 0,
            subtitles: None,
            subtitles_on: true,
            sub_scale: mechanics::SUB_SCALE_NORMAL,
            recent: Vec::new(),
            audio,
            sys_base: 0.0,
            sys_at: Instant::now(),
            buffering: false,
            need_preview: false,
            stepped: false,
            rng: seed,
            jump_sizes: [
                mechanics::JUMP_EXTRASHORT_S,
                mechanics::JUMP_SHORT_S,
                mechanics::JUMP_MEDIUM_S,
                mechanics::JUMP_LONG_S,
            ],
        }
    }

    pub fn has_audio_device(&self) -> bool {
        self.audio.is_some()
    }

    pub fn show_osd(&mut self, text: impl Into<String>) {
        self.osd = Some((text.into(), Instant::now()));
    }

    fn random_index(&mut self, len: usize) -> usize {
        // xorshift
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x % len as u64) as usize
    }

    // ----- playlist -------------------------------------------------------------------------

    /// Add files or folders; folders are searched for media files.
    pub fn add_paths(&mut self, paths: &[PathBuf]) -> Option<usize> {
        let mut first = None;
        let mut files = Vec::new();
        for p in paths {
            if p.is_dir() {
                collect_dir(p, &mut files, 0);
            } else {
                files.push(p.clone());
            }
        }
        for f in files {
            let loc = f.to_string_lossy().to_string();
            self.to_probe.push(loc.clone());
            self.playlist.push(Item::new(&loc));
            if first.is_none() {
                first = Some(self.playlist.len() - 1);
            }
        }
        self.order_dirty = true;
        first
    }

    pub fn add_location(&mut self, loc: &str) -> usize {
        self.playlist.push(Item::new(loc));
        self.order_dirty = true;
        self.playlist.len() - 1
    }

    pub fn open_and_play(&mut self, paths: &[PathBuf], ctx: &egui::Context) {
        if let Some(i) = self.add_paths(paths) {
            self.play_index(i, ctx);
        }
    }

    pub fn remove(&mut self, idx: usize, ctx: &egui::Context) {
        if idx >= self.playlist.len() {
            return;
        }
        self.playlist.remove(idx);
        self.order_dirty = true;
        match self.current {
            Some(c) if c == idx => {
                self.stop();
                self.current = None;
                let _ = ctx;
            }
            Some(c) if c > idx => self.current = Some(c - 1),
            _ => {}
        }
    }

    pub fn clear(&mut self) {
        self.stop();
        self.playlist.clear();
        self.current = None;
        self.order_dirty = true;
    }

    fn push_recent(&mut self, loc: &str) {
        self.recent.retain(|r| r != loc);
        self.recent.insert(0, loc.to_string());
        self.recent.truncate(mechanics::RECENT_MAX);
    }

    // ----- transport ------------------------------------------------------------------------

    pub fn play_index(&mut self, idx: usize, ctx: &egui::Context) {
        if idx >= self.playlist.len() {
            return;
        }
        self.session = None; // stops the old decoders
        self.ensure_order();
        self.order_pos = self.order.iter().position(|&x| x == idx);
        self.end_requested = false;
        self.current = Some(idx);
        self.error = None;
        self.frame = None;
        self.texture = None;
        self.stepped = false;
        self.aspect_index = 0;
        self.ab = (None, None);
        self.gen += 1;
        let loc = self.playlist[idx].location.clone();
        let audio = self.audio.as_ref().map(|a| (a.state.clone(), a.rate, a.channels));
        if let Some(a) = &self.audio {
            let mut st = a.state.lock().unwrap();
            st.reset(self.gen, 0.0);
            st.active = false;
            st.playing = false;
        }
        match media::open(&loc, self.gen, 0.0, audio, ctx.clone()) {
            Ok(session) => {
                let info = session.info.clone();
                self.playlist[idx].duration = Some(info.duration);
                if let Some(a) = &self.audio {
                    let mut st = a.state.lock().unwrap();
                    st.active = session.audio_ctl.is_some();
                    st.rate = self.rate;
                    st.volume = self.volume;
                    st.muted = self.muted;
                }
                session.set_rate(self.rate);
                self.info = Some(info);
                self.session = Some(session);
                self.state = State::Playing;
                self.sys_base = 0.0;
                self.sys_at = Instant::now();
                self.buffering = true;
                self.need_preview = true;
                self.subtitles = Subtitles::find_beside(&loc);
                self.push_recent(&loc);
            }
            Err(e) => {
                self.info = None;
                self.state = State::Stopped;
                self.error = Some(format!("Your input can't be opened:\n{}\n\n{}", loc, e));
            }
        }
    }

    pub fn play(&mut self, ctx: &egui::Context) {
        match self.state {
            State::Playing => {}
            State::Paused => self.resume(),
            State::Stopped => {
                if self.playlist.is_empty() {
                    return;
                }
                let i = self.current.unwrap_or(0).min(self.playlist.len() - 1);
                self.play_index(i, ctx);
            }
        }
    }

    pub fn toggle_pause(&mut self, ctx: &egui::Context) -> bool {
        match self.state {
            State::Playing => {
                self.pause();
                true
            }
            State::Paused => {
                self.resume();
                true
            }
            State::Stopped => {
                self.play(ctx);
                self.state != State::Stopped
            }
        }
    }

    pub fn pause(&mut self) {
        if self.state != State::Playing {
            return;
        }
        let t = self.position();
        self.sys_base = t;
        self.state = State::Paused;
        self.set_audio_playing(false);
    }

    pub fn resume(&mut self) {
        if self.state != State::Paused {
            return;
        }
        self.state = State::Playing;
        if self.stepped {
            // after stepping frame by frame, line the sound up with the picture again
            self.stepped = false;
            let t = self.frame_pts;
            self.seek(t);
        }
        self.sys_at = Instant::now();
        self.set_audio_playing(!self.buffering);
    }

    pub fn stop(&mut self) {
        self.session = None;
        self.state = State::Stopped;
        self.frame = None;
        self.texture = None;
        self.info = None;
        self.subtitles = None;
        self.sys_base = 0.0;
        if let Some(a) = &self.audio {
            let mut st = a.state.lock().unwrap();
            st.active = false;
            st.playing = false;
            st.chunks.clear();
        }
    }

    // ----- play order (rules from libvlccore playlist/thread.c, 0x140021270 and 0x140021670) ---

    /// Rebuild the play order: the playlist's own order, shuffled when Random is on
    /// (Fisher-Yates from the last item down: swap item i with item rand % (i+1)).
    /// The position is the current item's place before shuffling, as in the original.
    fn reset_order(&mut self) {
        let n = self.playlist.len();
        self.order = (0..n).collect();
        self.order_pos = self.current.filter(|&c| c < n);
        if self.random && n > 1 {
            for i in (1..n).rev() {
                let j = self.random_index(i + 1);
                self.order.swap(i, j);
            }
        }
        self.order_dirty = false;
    }

    fn ensure_order(&mut self) {
        if self.order_dirty || self.order.len() != self.playlist.len() {
            self.reset_order();
        }
    }

    /// Next / Previous asked for by the person: always wraps around the ends
    /// ("looping - restarting at beginning/end of node"), reshuffling first when Random is on.
    fn skip(&mut self, dir: i64, ctx: &egui::Context) {
        if let Some(idx) = self.skip_target(dir) {
            self.play_index(idx, ctx);
        }
    }

    /// Which item Next (dir 1) / Previous (dir -1) goes to; moves the order position.
    fn skip_target(&mut self, dir: i64) -> Option<usize> {
        if self.playlist.is_empty() {
            return None;
        }
        self.ensure_order();
        let n = self.order.len() as i64;
        let mut pos = self.order_pos.map(|p| p as i64).unwrap_or(-1) + dir;
        if pos >= n {
            if self.random {
                self.reset_order();
            }
            pos = 0;
        } else if pos < 0 {
            if self.random {
                self.reset_order();
            }
            pos = n - 1;
        }
        self.order_pos = Some(pos as usize);
        Some(self.order[pos as usize])
    }

    /// Which item plays after the current one ends by itself; None means stop.
    fn after_end_target(&mut self) -> Option<usize> {
        if self.playlist.is_empty() {
            return None;
        }
        self.ensure_order();
        let n = self.order.len();
        let mut pos = self.order_pos.map(|p| p + 1).unwrap_or(0);
        if pos >= n {
            if self.loop_mode != LoopMode::All {
                return None;
            }
            if self.random {
                self.reset_order();
            }
            pos = 0;
        }
        self.order_pos = Some(pos);
        Some(self.order[pos])
    }

    pub fn next(&mut self, ctx: &egui::Context) {
        self.skip(1, ctx);
    }

    pub fn prev(&mut self, ctx: &egui::Context) {
        self.skip(-1, ctx);
    }

    fn set_audio_playing(&self, playing: bool) {
        if let Some(a) = &self.audio {
            let mut st = a.state.lock().unwrap();
            st.playing = playing;
            if playing {
                st.clock_at = None;
            }
        }
    }

    // ----- time -----------------------------------------------------------------------------

    pub fn duration(&self) -> f64 {
        self.info.as_ref().map(|i| i.duration).unwrap_or(0.0)
    }

    pub fn is_active(&self) -> bool {
        self.state != State::Stopped && self.session.is_some()
    }

    fn audio_drives_clock(&self) -> bool {
        let Some(s) = &self.session else { return false };
        if s.audio_ctl.is_none() {
            return false;
        }
        let Some(a) = &self.audio else { return false };
        let st = a.state.lock().unwrap();
        !(st.eos && st.chunks.is_empty())
    }

    /// Where playback is now, in seconds.
    pub fn position(&self) -> f64 {
        if self.session.is_none() {
            return 0.0;
        }
        if self.stepped || self.state == State::Paused {
            return if self.stepped { self.frame_pts } else { self.sys_base };
        }
        if self.buffering {
            return self.sys_base;
        }
        if self.audio_drives_clock() {
            if let Some(a) = &self.audio {
                let st = a.state.lock().unwrap();
                if let Some(t) = heard_clock(&st) {
                    return t;
                }
                return self.sys_base;
            }
        }
        self.sys_base + self.sys_at.elapsed().as_secs_f64() * self.rate as f64
    }

    /// Go to `t` seconds. Like the original's time setting (libvlccore 0x14005f9d0) the time
    /// never goes below 0 and has no upper cap: a time past the end finishes the item.
    pub fn seek(&mut self, t: f64) {
        let Some(session) = &self.session else { return };
        let d = self.duration();
        if d > 0.0 && t >= d {
            self.end_requested = true;
            return;
        }
        let t = t.max(0.0);
        self.gen += 1;
        session.seek(self.gen, t);
        if let Some(a) = &self.audio {
            let mut st = a.state.lock().unwrap();
            st.reset(self.gen, t);
            st.playing = false;
        }
        self.sys_base = t;
        self.sys_at = Instant::now();
        self.buffering = true;
        self.need_preview = true;
        self.stepped = false;
    }

    pub fn jump(&mut self, delta: f64) {
        if !self.is_active() {
            return;
        }
        // time-offset = jump size × direction (libhotkeys case 0x12-0x19), then the position message
        let t = (self.position() + delta).max(0.0);
        self.seek(t);
        self.show_position_osd(t);
    }

    /// "elapsed / total", or only the elapsed time when the length isn't known (libhotkeys 0x140001b10).
    pub fn show_position_osd(&mut self, t: f64) {
        let d = self.duration();
        if d > 0.0 {
            self.show_osd(format!("{} / {}", fmt_time(t.min(d)), fmt_time(d)));
        } else {
            self.show_osd(fmt_time(t));
        }
    }

    pub fn seek_fraction(&mut self, f: f64) {
        let d = self.duration();
        if d > 0.0 {
            self.seek(f.clamp(0.0, 1.0) * d);
        }
    }

    /// Show the next picture while paused (the E key).
    pub fn frame_step(&mut self) {
        if !self.is_active() {
            return;
        }
        if self.state == State::Playing {
            self.pause();
        }
        let Some(session) = &self.session else { return };
        let Some(v) = &session.video else { return };
        let next = {
            let mut q = v.frames.lock().unwrap();
            q.retain(|f| f.gen == self.gen);
            q.pop_front()
        };
        if let Some(f) = next {
            self.frame_pts = f.pts;
            self.sys_base = f.pts;
            self.stepped = true;
            self.set_frame(f);
        }
        self.show_osd("Next frame");
    }

    // ----- speed and volume -----------------------------------------------------------------

    pub fn set_rate(&mut self, r: f32) {
        let pos = self.position();
        self.rate = r;
        self.sys_base = pos;
        self.sys_at = Instant::now();
        if let Some(s) = &self.session {
            s.set_rate(r);
        }
        if let Some(a) = &self.audio {
            a.state.lock().unwrap().rate = r;
        }
        self.show_osd(format!("Speed: {:.2}x", r));
    }

    pub fn faster(&mut self) {
        self.set_rate(mechanics::rate_faster(self.rate));
    }
    pub fn slower(&mut self) {
        self.set_rate(mechanics::rate_slower(self.rate));
    }
    pub fn faster_fine(&mut self) {
        self.set_rate(mechanics::rate_fine(self.rate, 1));
    }
    pub fn slower_fine(&mut self) {
        self.set_rate(mechanics::rate_fine(self.rate, -1));
    }
    pub fn normal_rate(&mut self) {
        self.set_rate(1.0);
    }

    pub fn set_volume(&mut self, v: f32, osd: bool) {
        self.volume = v.clamp(0.0, mechanics::VOLUME_MAX);
        if let Some(a) = &self.audio {
            let mut st = a.state.lock().unwrap();
            st.volume = self.volume;
        }
        if osd {
            self.show_osd(format!("Volume {}%", (self.volume * 100.0).round() as i64));
        }
    }

    /// Volume keys and the wheel (aout_VolumeUpdate, libvlccore 0x14006bc80): only the level
    /// changes; a muted player stays muted.
    pub fn volume_steps(&mut self, steps: i32) {
        let v = mechanics::volume_after_steps(self.volume, steps);
        self.set_volume(v, true);
    }

    pub fn set_muted(&mut self, m: bool) {
        self.muted = m;
        if let Some(a) = &self.audio {
            a.state.lock().unwrap().muted = m;
        }
    }

    /// Mute key (libhotkeys case 0x1c): muting, or a volume of 0, shows the mute icon;
    /// unmuting shows "Volume N%".
    pub fn toggle_mute(&mut self) {
        let m = !self.muted;
        self.set_muted(m);
        if m || self.volume <= 0.0 {
            self.osd = None;
            self.show_icon(OsdIcon::Mute);
        } else {
            self.osd_icon = None;
            self.show_osd(format!("Volume {}%", (self.volume * 100.0).round() as i64));
        }
    }

    pub fn show_icon(&mut self, icon: OsdIcon) {
        self.osd_icon = Some((icon, Instant::now()));
    }

    pub fn audio_level(&self) -> f32 {
        self.audio.as_ref().map(|a| a.state.lock().unwrap().level).unwrap_or(0.0)
    }

    /// First press sets A, second sets B, third clears the A→B loop.
    pub fn ab_loop(&mut self) {
        if !self.is_active() {
            return;
        }
        let t = self.position();
        self.ab = match self.ab {
            (None, _) => {
                self.show_osd("A→B loop: A set");
                (Some(t), None)
            }
            (Some(a), None) if t > a + 0.1 => {
                self.show_osd("A→B loop: on");
                (Some(a), Some(t))
            }
            (Some(a), None) => (Some(a), None),
            (Some(_), Some(_)) => {
                self.show_osd("A→B loop: off");
                (None, None)
            }
        };
    }

    pub fn cycle_loop(&mut self) {
        self.loop_mode = self.loop_mode.next();
        let label = match self.loop_mode {
            LoopMode::Off => "Off",
            LoopMode::All => "All",
            LoopMode::One => "One",
        };
        self.show_osd(format!("Loop: {}", label));
    }

    pub fn toggle_random(&mut self) {
        self.random = !self.random;
        self.order_dirty = true;
        self.show_osd(format!("Random: {}", if self.random { "On" } else { "Off" }));
    }

    pub fn cycle_aspect(&mut self) {
        self.aspect_index = (self.aspect_index + 1) % mechanics::ASPECT_RATIOS.len();
        self.show_osd(format!("Aspect ratio: {}", mechanics::ASPECT_RATIOS[self.aspect_index].0));
    }

    /// Width / height to show the picture at.
    pub fn display_aspect(&self) -> f32 {
        let forced = mechanics::ASPECT_RATIOS[self.aspect_index].1;
        if forced > 0.0 {
            return forced;
        }
        if let Some(f) = &self.frame {
            let par = self.info.as_ref().map(|i| if i.pixel_aspect > 0.0 { i.pixel_aspect } else { 1.0 }).unwrap_or(1.0);
            return f.size[0] as f32 * par / f.size[1].max(1) as f32;
        }
        self.info.as_ref().map(|i| i.display_aspect()).unwrap_or(16.0 / 9.0)
    }

    // ----- every frame ----------------------------------------------------------------------

    fn set_frame(&mut self, f: media::VideoFrame) {
        let img = Arc::new(ColorImage::new([f.width, f.height], f.pixels));
        self.frame_pts = f.pts;
        self.frame = Some(img);
        self.frame_dirty = true; // uploaded by `texture()`
    }

    pub fn texture(&mut self, ctx: &egui::Context) -> Option<TextureHandle> {
        let img = self.frame.as_ref()?.clone();
        if self.frame_dirty || self.texture.is_none() {
            self.frame_dirty = false;
            match &mut self.texture {
                Some(t) if t.size() == img.size => t.set(egui::ImageData::Color(img), TextureOptions::LINEAR),
                _ => {
                    self.texture = Some(ctx.load_texture("video", egui::ImageData::Color(img), TextureOptions::LINEAR));
                }
            }
        }
        self.texture.clone()
    }

    /// Advance playback: pick the picture to show, notice the end of the file.
    pub fn update(&mut self, ctx: &egui::Context) {
        if let Some((_, at)) = &self.osd {
            if at.elapsed().as_secs_f64() > mechanics::OSD_SECONDS {
                self.osd = None;
            }
        }
        if let Some((_, at)) = &self.osd_icon {
            if at.elapsed().as_secs_f64() > mechanics::OSD_ICON_SECONDS {
                self.osd_icon = None;
            }
        }
        if self.end_requested && self.session.is_some() {
            self.end_requested = false;
            self.on_end(ctx);
            return;
        }
        // durations read in the background
        if !self.to_probe.is_empty() {
            let list = std::mem::take(&mut self.to_probe);
            media::probe_durations(list, self.durations_tx.clone(), ctx.clone());
        }
        while let Ok((loc, d)) = self.durations_rx.try_recv() {
            for it in self.playlist.iter_mut().filter(|it| it.location == loc && it.duration.is_none()) {
                it.duration = Some(d);
            }
        }
        let Some(session) = &self.session else { return };
        if let Some(e) = session.error() {
            if self.frame.is_none() {
                self.error = Some(format!("This picture can't be shown: {}", e));
            }
        }
        let has_video = session.video.is_some();
        let has_audio = session.audio_ctl.is_some() && self.audio.is_some();

        // still filling up after opening or seeking?
        if self.buffering {
            let video_ready = match &session.video {
                Some(v) => {
                    let q = v.frames.lock().unwrap();
                    q.iter().any(|f| f.gen == self.gen) || session.video_eos()
                }
                None => true,
            };
            let audio_ready = if has_audio {
                let st = self.audio.as_ref().unwrap().state.lock().unwrap();
                st.queued_seconds() > 0.08 || st.eos || session.audio_ctl.as_ref().map(|c| c.eos.load(std::sync::atomic::Ordering::SeqCst)).unwrap_or(true)
            } else {
                true
            };
            if video_ready && audio_ready {
                self.buffering = false;
                self.sys_at = Instant::now();
                if self.state == State::Playing {
                    self.set_audio_playing(true);
                }
            }
        }

        let now = self.position();
        let gen = self.gen;
        let mut chosen = None;
        if let Some(v) = &session.video {
            let mut q = v.frames.lock().unwrap();
            while q.front().map(|f| f.gen != gen).unwrap_or(false) {
                q.pop_front();
            }
            if self.need_preview && self.state != State::Playing {
                chosen = q.pop_front();
            } else if self.state == State::Playing && !self.buffering {
                while let Some(f) = q.front() {
                    if f.pts <= now + 0.008 || self.need_preview {
                        chosen = q.pop_front();
                        self.need_preview = false;
                    } else {
                        break;
                    }
                }
            }
        }
        if let Some(f) = chosen {
            self.need_preview = false;
            self.set_frame(f);
        }
        // A→B loop
        if let (Some(a), Some(b)) = self.ab {
            if self.state == State::Playing && !self.buffering && now >= b {
                self.seek(a);
                return;
            }
        }
        let Some(session) = &self.session else { return };

        // end of the file?
        if self.state == State::Playing && !self.buffering {
            let video_done = !has_video
                || (session.video_eos()
                    && session.video.as_ref().map(|v| v.frames.lock().unwrap().iter().all(|f| f.gen != gen)).unwrap_or(true));
            let audio_done = if has_audio {
                let st = self.audio.as_ref().unwrap().state.lock().unwrap();
                st.eos && st.chunks.is_empty()
            } else {
                true
            };
            let past_end = self.duration() > 0.0 && now > self.duration() + 1.0;
            if (video_done && audio_done) || (video_done && past_end) {
                self.on_end(ctx);
            }
        }
    }

    /// The current item finished by itself (NextItem with no request, libvlccore 0x140021670):
    /// Repeat One plays it again; otherwise the next item in the play order; past the last
    /// item, Loop All starts over (reshuffled when Random is on), else playback stops.
    fn on_end(&mut self, ctx: &egui::Context) {
        if self.loop_mode == LoopMode::One && self.current.is_some() {
            self.seek(0.0);
            return;
        }
        match self.after_end_target() {
            Some(idx) => self.play_index(idx, ctx),
            None => {
                self.stop();
                if self.quit_at_end {
                    self.want_quit = true;
                }
            }
        }
    }

    /// One line about the clock and queues, for testing.
    pub fn debug_line(&self) -> String {
        let (q, clock, eos, playing) = match &self.audio {
            Some(a) => {
                let st = a.state.lock().unwrap();
                (st.queued_seconds(), heard_clock(&st).unwrap_or(-1.0), st.eos, st.playing)
            }
            None => (0.0, -1.0, false, false),
        };
        let vq = self
            .session
            .as_ref()
            .and_then(|s| s.video.as_ref())
            .map(|v| v.frames.lock().unwrap().len())
            .unwrap_or(0);
        format!(
            "state={:?} pos={:.3} frame_pts={:.3} rate={} buffering={} audio_q={:.2}s audio_clock={:.3} audio_eos={} audio_playing={} video_q={} item={:?}",
            self.state,
            self.position(),
            self.frame_pts,
            self.rate,
            self.buffering,
            q,
            clock,
            eos,
            playing,
            vq,
            self.current
        )
    }

    pub fn current_title(&self) -> Option<String> {
        self.current.and_then(|i| self.playlist.get(i)).map(|it| it.title.clone())
    }

    pub fn subtitle_text(&self) -> Option<String> {
        if !self.subtitles_on {
            return None;
        }
        let subs = self.subtitles.as_ref()?;
        subs.at(self.position()).map(|s| s.to_string())
    }
}

fn collect_dir(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    for p in entries {
        if p.is_dir() {
            collect_dir(&p, out, depth + 1);
        } else if is_media(&p) {
            out.push(p);
        }
    }
}

/// "MM:SS", or "H:MM:SS" past an hour.
pub fn fmt_time(t: f64) -> String {
    let t = if t.is_finite() { t.max(0.0) } else { 0.0 };
    let s = t.floor() as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

/// Read "1:23:45", "83:10", "90" (seconds) into seconds.
pub fn parse_time(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut total = 0.0;
    for p in parts {
        let v: f64 = p.trim().parse().ok()?;
        if v < 0.0 {
            return None;
        }
        total = total * 60.0 + v;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(n: usize) -> Player {
        let mut p = Player::new(false);
        for i in 0..n {
            p.add_location(&format!("item{i}.mp4"));
        }
        p
    }

    #[test]
    fn next_and_previous_wrap_like_the_original() {
        let mut p = player(3);
        p.current = Some(2);
        assert_eq!(p.skip_target(1), Some(0)); // last → first, loop off
        p.current = Some(0);
        p.order_dirty = true;
        assert_eq!(p.skip_target(-1), Some(2)); // first → last
    }

    #[test]
    fn end_of_item_stops_unless_loop_all() {
        let mut p = player(3);
        p.current = Some(1);
        p.order_dirty = true;
        assert_eq!(p.after_end_target(), Some(2));
        assert_eq!(p.after_end_target(), None); // past the last: stop
        p.loop_mode = LoopMode::All;
        p.order_pos = Some(2);
        assert_eq!(p.after_end_target(), Some(0)); // loop all starts over
    }

    #[test]
    fn random_plays_each_item_once() {
        let mut p = player(8);
        p.random = true;
        p.current = None;
        p.order_dirty = true;
        let mut seen = Vec::new();
        while let Some(i) = p.after_end_target() {
            seen.push(i);
            assert!(seen.len() <= 8);
        }
        seen.sort();
        assert_eq!(seen, (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn time_format_matches_secstotimestr() {
        assert_eq!(fmt_time(65.9), "01:05");
        assert_eq!(fmt_time(3725.0), "1:02:05");
    }
}
