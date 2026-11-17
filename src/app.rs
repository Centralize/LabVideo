//! The main window: menus, video area, time bar, control buttons, playlist and dialogs.

use std::path::PathBuf;
use std::time::Instant;

use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Key, Layout, Rect, RichText, Sense, Shape, Stroke, StrokeKind,
    Ui, Vec2,
};

use crate::icons::{self, Icon};
use crate::mechanics;
use crate::player::{fmt_time, parse_time, LoopMode, OsdIcon, Player, State};
use crate::settings::Settings;
use crate::subtitles::Subtitles;
use crate::APP_NAME;

// Colours of the light window theme.
const BG: Color32 = Color32::from_rgb(240, 240, 240);
const MENU_BG: Color32 = Color32::from_rgb(255, 255, 255);
const TEXT: Color32 = Color32::from_rgb(30, 30, 30);
const ICON: Color32 = Color32::from_rgb(52, 52, 52);
const ACCENT: Color32 = Color32::from_rgb(38, 110, 210);
const HOVER_BG: Color32 = Color32::from_rgb(229, 241, 251);
const HOVER_BORDER: Color32 = Color32::from_rgb(204, 228, 247);

#[derive(Clone, Debug)]
enum Action {
    OpenFile,
    OpenFiles,
    OpenFolder,
    OpenNetwork,
    OpenRecent(String),
    ClearRecent,
    SavePlaylist,
    Quit,
    ToggleQuitAtEnd,
    PlayPause,
    Stop,
    Prev,
    Next,
    Faster,
    FasterFine,
    NormalSpeed,
    SlowerFine,
    Slower,
    Jump(f64),
    JumpToTime,
    FrameStep,
    VolumeSteps(i32),
    Mute,
    Fullscreen,
    LeaveFullscreen,
    TogglePlaylist,
    ToggleMinimal,
    ToggleStatusBar,
    ToggleOnTop,
    Loop,
    Random,
    Aspect(Option<usize>),
    Snapshot,
    AddSubtitle,
    SubtitlesOn(bool),
    SubtitleDelay(f64),
    /// 0 = normal size, +1 bigger, -1 smaller.
    SubtitleScale(i32),
    MediaInfo,
    Preferences,
    Extended,
    Shortcuts,
    About,
    ShowPosition,
    AbLoop,
    ToggleAdvanced,
    PlayIndex(usize),
    RemoveIndex(usize),
    ClearPlaylist,
    AddToPlaylist,
}

/// Options for trying the player from the command line (used while building and checking).
#[derive(Default, Clone)]
pub struct TestOptions {
    /// Seek here right after opening.
    pub start: Option<f64>,
    /// Take the screenshot after this many frames instead of 120.
    pub frames: Option<u32>,
    /// "frame:action,frame:action" — e.g. "60:seek=10,90:pause,120:faster".
    pub keys: String,
}

pub struct PlayerApp {
    test: TestOptions,
    player: Player,
    settings: Settings,
    screenshot: Option<PathBuf>,
    frames_drawn: u32,
    screenshot_requested: bool,
    fullscreen: bool,
    minimal: bool,
    on_top: bool,
    advanced: bool,
    show_playlist: bool,
    show_remaining: bool,
    drag_seek: Option<f64>,
    last_drag_seek: Instant,
    last_mouse_move: Instant,
    last_pointer: Option<egui::Pos2>,
    selected: Option<usize>,
    search: String,
    // dialogs
    net_open: bool,
    net_url: String,
    goto_open: bool,
    goto_text: String,
    info_open: bool,
    prefs_open: bool,
    ext_open: bool,
    keys_open: bool,
    about_open: bool,
    actions: Vec<Action>,
    /// Actions from the keyboard and the mouse wheel: like the original's hotkeys they show
    /// on-screen messages; menu and button actions don't.
    key_actions: Vec<Action>,
    /// Snapshot preview shown top-left for a few seconds after saving.
    snap_preview: Option<(egui::TextureHandle, Instant)>,
    last_title: String,
}

pub fn window_icon() -> egui::IconData {
    // a small orange play badge, drawn by hand
    let n = 64usize;
    let mut rgba = vec![0u8; n * n * 4];
    let c = (n as f32 - 1.0) / 2.0;
    for y in 0..n {
        for x in 0..n {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let d = (dx * dx + dy * dy).sqrt();
            let i = (y * n + x) * 4;
            if d <= 30.0 {
                let t = (d / 30.0).clamp(0.0, 1.0);
                let col = icons::lerp_color(Color32::from_rgb(255, 176, 64), Color32::from_rgb(232, 96, 20), t);
                // play triangle
                let tx = x as f32 - 22.0;
                let inside = tx >= 0.0 && tx <= 26.0 && (dy.abs() <= (26.0 - tx) * 0.62);
                let col = if inside { Color32::WHITE } else { col };
                rgba[i] = col.r();
                rgba[i + 1] = col.g();
                rgba[i + 2] = col.b();
                rgba[i + 3] = 255;
            }
        }
    }
    egui::IconData { rgba, width: n as u32, height: n as u32 }
}

impl PlayerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>, screenshot: Option<PathBuf>, test: TestOptions) -> Self {
        let ctx = &cc.egui_ctx;
        setup_style(ctx);
        let checking = screenshot.is_some();
        let settings = if checking { Settings::default() } else { Settings::load() };
        let show_remaining = settings.show_remaining;
        let mut player = Player::new(!checking || !files.is_empty());
        player.set_volume(settings.volume, false);
        player.jump_sizes = settings.jump_sizes;
        player.recent = settings.recent.clone();
        player.loop_mode = match settings.loop_mode {
            1 => LoopMode::All,
            2 => LoopMode::One,
            _ => LoopMode::Off,
        };
        player.random = settings.random;
        player.osd = None;
        if !files.is_empty() {
            player.open_and_play(&files, ctx);
            if let Some(t) = test.start {
                player.seek(t);
            }
        }
        Self {
            test,
            player,
            settings,
            screenshot,
            frames_drawn: 0,
            screenshot_requested: false,
            fullscreen: false,
            minimal: false,
            on_top: false,
            advanced: false,
            show_playlist: false,
            show_remaining,
            drag_seek: None,
            last_drag_seek: Instant::now(),
            last_mouse_move: Instant::now(),
            last_pointer: None,
            selected: None,
            search: String::new(),
            net_open: false,
            net_url: String::new(),
            goto_open: false,
            goto_text: String::new(),
            info_open: false,
            prefs_open: false,
            ext_open: false,
            keys_open: false,
            about_open: false,
            actions: Vec::new(),
            key_actions: Vec::new(),
            snap_preview: None,
            last_title: String::new(),
        }
    }

    fn save_settings(&mut self) {
        if self.screenshot.is_some() {
            return;
        }
        self.settings.volume = self.player.volume;
        self.settings.recent = self.player.recent.clone();
        self.settings.jump_sizes = self.player.jump_sizes;
        self.settings.loop_mode = match self.player.loop_mode {
            LoopMode::Off => 0,
            LoopMode::All => 1,
            LoopMode::One => 2,
        };
        self.settings.random = self.player.random;
        self.settings.save();
    }

    // ----- actions --------------------------------------------------------------------------

    fn apply(&mut self, action: Action, ctx: &egui::Context) {
        let p = &mut self.player;
        let js = p.jump_sizes;
        match action {
            Action::OpenFile => {
                if let Some(f) = media_dialog().pick_file() {
                    p.open_and_play(&[f], ctx);
                }
            }
            Action::OpenFiles => {
                if let Some(fs) = media_dialog().set_title("Select one or more files to open").pick_files() {
                    p.open_and_play(&fs, ctx);
                }
            }
            Action::OpenFolder => {
                if let Some(d) = rfd::FileDialog::new().set_title("Open Folder").pick_folder() {
                    p.open_and_play(&[d], ctx);
                }
            }
            Action::AddToPlaylist => {
                if let Some(fs) = media_dialog().set_title("Add files to the playlist").pick_files() {
                    p.add_paths(&fs);
                }
            }
            Action::OpenNetwork => {
                self.net_open = true;
            }
            Action::OpenRecent(loc) => {
                if loc.contains("://") {
                    let i = p.add_location(&loc);
                    p.play_index(i, ctx);
                } else {
                    p.open_and_play(&[PathBuf::from(loc)], ctx);
                }
            }
            Action::ClearRecent => p.recent.clear(),
            Action::SavePlaylist => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Save playlist as...")
                    .add_filter("M3U playlist", &["m3u", "m3u8"])
                    .set_file_name("playlist.m3u")
                    .save_file()
                {
                    let mut out = String::from("#EXTM3U\n");
                    for it in &p.playlist {
                        let d = it.duration.map(|d| d.round() as i64).unwrap_or(-1);
                        out += &format!("#EXTINF:{},{}\n{}\n", d, it.title, it.location);
                    }
                    let _ = std::fs::write(path, out);
                }
            }
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::ToggleQuitAtEnd => p.quit_at_end = !p.quit_at_end,
            Action::PlayPause => {
                if p.playlist.is_empty() && p.state == State::Stopped {
                    self.actions.push(Action::OpenFile);
                } else {
                    // libhotkeys case 2: the play icon when it was paused, the pause icon otherwise
                    let was_paused = p.state == State::Paused;
                    p.toggle_pause(ctx);
                    p.osd = None;
                    p.show_icon(if was_paused { OsdIcon::Play } else { OsdIcon::Pause });
                }
            }
            Action::Stop => p.stop(),
            Action::Prev => {
                p.show_osd("Previous");
                p.prev(ctx);
            }
            Action::Next => {
                p.show_osd("Next");
                p.next(ctx);
            }
            Action::Faster => p.faster(),
            Action::FasterFine => p.faster_fine(),
            Action::NormalSpeed => p.normal_rate(),
            Action::SlowerFine => p.slower_fine(),
            Action::Slower => p.slower(),
            Action::Jump(d) => {
                let size = match d.abs() as i64 {
                    1 => js[0],
                    2 => js[1],
                    3 => js[2],
                    _ => js[3],
                };
                p.jump(size * d.signum());
            }
            Action::JumpToTime => {
                if p.is_active() {
                    self.goto_text = fmt_time(p.position());
                    self.goto_open = true;
                }
            }
            Action::FrameStep => p.frame_step(),
            Action::VolumeSteps(s) => p.volume_steps(s),
            Action::Mute => p.toggle_mute(),
            Action::Fullscreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
                self.last_mouse_move = Instant::now();
            }
            Action::LeaveFullscreen => {
                if self.fullscreen {
                    self.fullscreen = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                }
            }
            Action::TogglePlaylist => self.show_playlist = !self.show_playlist,
            Action::ToggleMinimal => self.minimal = !self.minimal,
            Action::ToggleStatusBar => self.settings.status_bar = !self.settings.status_bar,
            Action::ToggleOnTop => {
                self.on_top = !self.on_top;
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }));
            }
            Action::Loop => p.cycle_loop(),
            Action::Random => p.toggle_random(),
            Action::Aspect(None) => p.cycle_aspect(),
            Action::Aspect(Some(i)) => {
                p.aspect_index = i.min(mechanics::ASPECT_RATIOS.len() - 1);
                let name = mechanics::ASPECT_RATIOS[p.aspect_index].0;
                p.show_osd(format!("Aspect ratio: {}", name));
            }
            Action::Snapshot => {
                // VoutSaveSnapshot: the saved file's path as a message, and a preview top-left
                match save_snapshot(p) {
                    Ok(path) => {
                        p.show_osd(path);
                        if let Some(img) = p.frame.clone() {
                            let tex = ctx.load_texture("snapshot-preview", egui::ImageData::Color(img), egui::TextureOptions::LINEAR);
                            self.snap_preview = Some((tex, Instant::now()));
                        }
                    }
                    Err(msg) => p.show_osd(msg),
                }
            }
            Action::AddSubtitle => {
                if let Some(f) = rfd::FileDialog::new()
                    .set_title("Open subtitle file")
                    .add_filter("Subtitles (*.srt)", &["srt"])
                    .pick_file()
                {
                    match Subtitles::load(&f) {
                        Some(s) => {
                            p.show_osd(format!("Subtitle track: {}", s.name));
                            p.subtitles = Some(s);
                            p.subtitles_on = true;
                        }
                        None => p.show_osd("Subtitle file could not be read"),
                    }
                }
            }
            Action::SubtitlesOn(on) => {
                p.subtitles_on = on;
                p.show_osd(if on { "Subtitle track: on" } else { "Subtitle track: disabled" });
            }
            Action::SubtitleDelay(dir) => {
                // libhotkeys case 0x32/0x33: ±50 ms, or "No active subtitle"
                match &mut p.subtitles {
                    Some(s) if p.subtitles_on => {
                        s.delay += dir.signum() * mechanics::SUB_DELAY_STEP_S;
                        let ms = (s.delay * 1000.0).round() as i64;
                        p.show_osd(format!("Subtitle delay {} ms", ms));
                    }
                    _ => p.show_osd("No active subtitle"),
                }
            }
            Action::SubtitleScale(dir) => {
                // libhotkeys cases 0x3e-0x40: normal / bigger / smaller
                p.sub_scale = match dir {
                    0 => mechanics::SUB_SCALE_NORMAL,
                    d => mechanics::sub_scale_step(p.sub_scale, d > 0),
                };
                p.show_osd(format!("Subtitle text scale {}%", p.sub_scale));
            }
            Action::MediaInfo => self.info_open = true,
            Action::Preferences => self.prefs_open = true,
            Action::Extended => self.ext_open = !self.ext_open,
            Action::Shortcuts => self.keys_open = true,
            Action::About => self.about_open = true,
            Action::ShowPosition => {
                if p.is_active() {
                    let t = p.position();
                    p.show_position_osd(t);
                }
            }
            Action::AbLoop => p.ab_loop(),
            Action::ToggleAdvanced => self.advanced = !self.advanced,
            Action::PlayIndex(i) => p.play_index(i, ctx),
            Action::RemoveIndex(i) => {
                p.remove(i, ctx);
                self.selected = None;
            }
            Action::ClearPlaylist => {
                p.clear();
                self.selected = None;
            }
        }
    }

    // ----- keyboard -------------------------------------------------------------------------

    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Text(t) => {
                    let a = match t.as_str() {
                        "+" => Some(Action::Faster),
                        "-" => Some(Action::Slower),
                        "=" => Some(Action::NormalSpeed),
                        "]" => Some(Action::FasterFine),
                        "[" => Some(Action::SlowerFine),
                        _ => None,
                    };
                    if let Some(a) = a {
                        self.key_actions.push(a);
                    }
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    let ctrl = modifiers.ctrl || modifiers.command;
                    let alt = modifiers.alt;
                    let shift = modifiers.shift;
                    let a = match key {
                        Key::Space if !ctrl && !alt => Some(Action::PlayPause),
                        Key::F if !ctrl && !alt && !shift => Some(Action::Fullscreen),
                        Key::F11 => Some(Action::Fullscreen),
                        Key::Escape => Some(Action::LeaveFullscreen),
                        Key::N if !ctrl && !alt && !shift => Some(Action::Next),
                        Key::P if !ctrl && !alt && !shift => Some(Action::Prev),
                        Key::S if shift && !ctrl && !alt => Some(Action::Snapshot),
                        Key::S if !ctrl && !alt => Some(Action::Stop),
                        Key::T if ctrl => Some(Action::JumpToTime),
                        Key::T if !alt && !shift => Some(Action::ShowPosition),
                        Key::E if !ctrl && !alt && !shift => Some(Action::FrameStep),
                        Key::M if !ctrl && !alt && !shift => Some(Action::Mute),
                        Key::R if !ctrl && !alt && !shift => Some(Action::Random),
                        Key::L if ctrl => Some(Action::TogglePlaylist),
                        Key::L if !alt && !shift => Some(Action::Loop),
                        Key::A if !ctrl && !alt && !shift => Some(Action::Aspect(None)),
                        Key::H if ctrl => Some(Action::ToggleMinimal),
                        Key::G if !ctrl && !alt && !shift => Some(Action::SubtitleDelay(-0.05)),
                        Key::H if !ctrl && !alt && !shift => Some(Action::SubtitleDelay(0.05)),
                        Key::V if !ctrl && !alt && !shift => Some(Action::SubtitlesOn(!self.player.subtitles_on)),
                        Key::Q if ctrl => Some(Action::Quit),
                        Key::O if ctrl && shift => Some(Action::OpenFiles),
                        Key::O if ctrl => Some(Action::OpenFile),
                        Key::F if ctrl => Some(Action::OpenFolder),
                        Key::N if ctrl => Some(Action::OpenNetwork),
                        Key::I if ctrl => Some(Action::MediaInfo),
                        Key::P if ctrl => Some(Action::Preferences),
                        Key::E if ctrl => Some(Action::Extended),
                        Key::Y if ctrl => Some(Action::SavePlaylist),
                        Key::Num0 if ctrl => Some(Action::SubtitleScale(0)),
                        Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7 | Key::Num8 | Key::Num9
                            if ctrl =>
                        {
                            // Ctrl+1..Ctrl+9 open the recent items (Open Recent Media menu, 0x1400069b0)
                            let n = match key {
                                Key::Num1 => 0,
                                Key::Num2 => 1,
                                Key::Num3 => 2,
                                Key::Num4 => 3,
                                Key::Num5 => 4,
                                Key::Num6 => 5,
                                Key::Num7 => 6,
                                Key::Num8 => 7,
                                _ => 8,
                            };
                            self.player.recent.get(n).cloned().map(Action::OpenRecent)
                        }
                        Key::F1 => Some(Action::Shortcuts),
                        Key::ArrowUp if ctrl => Some(Action::VolumeSteps(1)),
                        Key::ArrowDown if ctrl => Some(Action::VolumeSteps(-1)),
                        Key::ArrowLeft | Key::ArrowRight => {
                            let dir = if key == Key::ArrowLeft { -1.0 } else { 1.0 };
                            if ctrl && alt {
                                Some(Action::Jump(4.0 * dir))
                            } else if ctrl {
                                Some(Action::Jump(3.0 * dir))
                            } else if alt {
                                Some(Action::Jump(2.0 * dir))
                            } else if shift {
                                Some(Action::Jump(1.0 * dir))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(a) = a {
                        self.key_actions.push(a);
                    }
                }
                _ => {}
            }
        }
    }

    // ----- menus ----------------------------------------------------------------------------

    fn menu_bar(&mut self, ui: &mut Ui) {
        let acts = &mut self.actions;
        let p = &self.player;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Media", |ui| {
                item(ui, acts, "Open File...", "Ctrl+O", Action::OpenFile);
                item(ui, acts, "Open Multiple Files...", "Ctrl+Shift+O", Action::OpenFiles);
                item(ui, acts, "Open Folder...", "Ctrl+F", Action::OpenFolder);
                item(ui, acts, "Open Network Stream...", "Ctrl+N", Action::OpenNetwork);
                // Open Recent Media (libqt_plugin 0x1400069b0): the first 10 items as "N: name",
                // Ctrl+1..Ctrl+9 on the first nine, long names cut in the middle, then Clear;
                // greyed out when there is nothing.
                ui.add_enabled_ui(!p.recent.is_empty(), |ui| {
                    ui.menu_button("Open Recent Media", |ui| {
                        for (i, r) in p.recent.iter().take(mechanics::RECENT_MENU_MAX).enumerate() {
                            let label = format!("{}: {}", i + 1, elide_middle(r, 60));
                            let mut b = egui::Button::new(label);
                            if i < 9 {
                                b = b.shortcut_text(format!("Ctrl+{}", i + 1));
                            }
                            if ui.add(b).on_hover_text(r).clicked() {
                                acts.push(Action::OpenRecent(r.clone()));
                            }
                        }
                        ui.separator();
                        if ui.button("Clear").clicked() {
                            acts.push(Action::ClearRecent);
                        }
                    });
                });
                item(ui, acts, "Save Playlist to File...", "Ctrl+Y", Action::SavePlaylist);
                ui.separator();
                let b = egui::Button::new("Quit at the end of playlist").selected(p.quit_at_end);
                if ui.add(b).clicked() {
                    acts.push(Action::ToggleQuitAtEnd);
                }
                item(ui, acts, "Quit", "Ctrl+Q", Action::Quit);
            });
            ui.menu_button("Playback", |ui| {
                ui.menu_button("Speed", |ui| {
                    item(ui, acts, "Faster", "+", Action::Faster);
                    item(ui, acts, "Faster (fine)", "]", Action::FasterFine);
                    item(ui, acts, "Normal Speed", "=", Action::NormalSpeed);
                    item(ui, acts, "Slower (fine)", "[", Action::SlowerFine);
                    item(ui, acts, "Slower", "-", Action::Slower);
                });
                ui.separator();
                item(ui, acts, "Jump Forward", "Alt+Right", Action::Jump(2.0));
                item(ui, acts, "Jump Backward", "Alt+Left", Action::Jump(-2.0));
                item(ui, acts, "Jump to Specific Time", "Ctrl+T", Action::JumpToTime);
                ui.separator();
                let play_label = if p.state == State::Playing { "Pause" } else { "Play" };
                item(ui, acts, play_label, "Space", Action::PlayPause);
                item(ui, acts, "Stop", "S", Action::Stop);
                item(ui, acts, "Previous", "P", Action::Prev);
                item(ui, acts, "Next", "N", Action::Next);
                item(ui, acts, "Next Frame", "E", Action::FrameStep);
            });
            ui.menu_button("Audio", |ui| {
                ui.menu_button("Audio Track", |ui| {
                    let has = p.info.as_ref().map(|i| i.has_audio).unwrap_or(false);
                    ui.add_enabled(false, egui::Button::new(if has { "Track 1" } else { "Disable" }).selected(true));
                });
                ui.menu_button("Audio Device", |ui| {
                    ui.add_enabled(false, egui::Button::new(if p.has_audio_device() { "Default device" } else { "No device found" }).selected(true));
                });
                ui.separator();
                item(ui, acts, "Increase Volume", "Ctrl+Up", Action::VolumeSteps(1));
                item(ui, acts, "Decrease Volume", "Ctrl+Down", Action::VolumeSteps(-1));
                let mute = egui::Button::new("Mute").shortcut_text("M").selected(p.muted);
                if ui.add(mute).clicked() {
                    acts.push(Action::Mute);
                }
            });
            ui.menu_button("Video", |ui| {
                let fs = egui::Button::new("Fullscreen").shortcut_text("F");
                if ui.add(fs).clicked() {
                    acts.push(Action::Fullscreen);
                }
                ui.menu_button("Aspect Ratio", |ui| {
                    for (i, (name, _)) in mechanics::ASPECT_RATIOS.iter().enumerate() {
                        if ui.add(egui::Button::new(*name).selected(p.aspect_index == i)).clicked() {
                            acts.push(Action::Aspect(Some(i)));
                        }
                    }
                });
                ui.separator();
                item(ui, acts, "Take Snapshot", "Shift+S", Action::Snapshot);
            });
            ui.menu_button("Subtitle", |ui| {
                item(ui, acts, "Add Subtitle File...", "", Action::AddSubtitle);
                ui.menu_button("Sub Track", |ui| {
                    let has = p.subtitles.is_some();
                    if ui.add(egui::Button::new("Disable").selected(!p.subtitles_on || !has)).clicked() {
                        acts.push(Action::SubtitlesOn(false));
                    }
                    if let Some(s) = &p.subtitles {
                        if ui.add(egui::Button::new(s.name.clone()).selected(p.subtitles_on)).clicked() {
                            acts.push(Action::SubtitlesOn(true));
                        }
                    }
                });
                ui.separator();
                item(ui, acts, "Subtitle delay -50 ms", "G", Action::SubtitleDelay(-0.05));
                item(ui, acts, "Subtitle delay +50 ms", "H", Action::SubtitleDelay(0.05));
            });
            ui.menu_button("Tools", |ui| {
                item(ui, acts, "Effects and Filters", "Ctrl+E", Action::Extended);
                item(ui, acts, "Media Information", "Ctrl+I", Action::MediaInfo);
                ui.separator();
                item(ui, acts, "Preferences", "Ctrl+P", Action::Preferences);
            });
            ui.menu_button("View", |ui| {
                let b = egui::Button::new("Playlist").shortcut_text("Ctrl+L").selected(self.show_playlist);
                if ui.add(b).clicked() {
                    acts.push(Action::TogglePlaylist);
                }
                let b = egui::Button::new("Always on top").selected(self.on_top);
                if ui.add(b).clicked() {
                    acts.push(Action::ToggleOnTop);
                }
                let b = egui::Button::new("Minimal Interface").shortcut_text("Ctrl+H").selected(self.minimal);
                if ui.add(b).clicked() {
                    acts.push(Action::ToggleMinimal);
                }
                let b = egui::Button::new("Advanced Controls").selected(self.advanced);
                if ui.add(b).clicked() {
                    acts.push(Action::ToggleAdvanced);
                }
                let b = egui::Button::new("Fullscreen Interface").shortcut_text("F11").selected(self.fullscreen);
                if ui.add(b).clicked() {
                    acts.push(Action::Fullscreen);
                }
                let b = egui::Button::new("Status Bar").selected(self.settings.status_bar);
                if ui.add(b).clicked() {
                    acts.push(Action::ToggleStatusBar);
                }
            });
            ui.menu_button("Help", |ui| {
                item(ui, acts, "Keyboard Shortcuts", "F1", Action::Shortcuts);
                ui.separator();
                item(ui, acts, "About", "Shift+F1", Action::About);
            });
        });
    }

    // ----- controls -------------------------------------------------------------------------

    fn time_bar(&mut self, ui: &mut Ui) {
        let active = self.player.is_active();
        let dur = self.player.duration();
        let pos = self.drag_seek.map(|f| f * dur).unwrap_or_else(|| self.player.position());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let left = if active { fmt_time(pos) } else { "--:--".into() };
            ui.add_sized([46.0, 20.0], egui::Label::new(RichText::new(left).color(TEXT).size(12.5)));
            let right_w = 52.0;
            let w = (ui.available_width() - right_w - 6.0).max(40.0);
            let frac = if dur > 0.0 { (pos / dur).clamp(0.0, 1.0) } else { 0.0 };
            if let Some(new) = seek_slider(ui, w, frac, active && dur > 0.0, dur) {
                match new {
                    SeekEvent::Drag(f) => {
                        self.drag_seek = Some(f);
                        if self.last_drag_seek.elapsed().as_millis() > 180 {
                            self.player.seek_fraction(f);
                            self.last_drag_seek = Instant::now();
                        }
                    }
                    SeekEvent::Release(f) => {
                        self.player.seek_fraction(f);
                        self.drag_seek = None;
                    }
                }
            }
            // TimeLabel (libqt_plugin 0x1400c5e80): "--:--" when idle or the length is unknown,
            // else the total, or "-" + (whole length − whole elapsed seconds) when toggled
            let right = if !active || dur <= 0.0 {
                "--:--".to_string()
            } else if self.show_remaining {
                let rem = dur.floor() as i64 - pos.max(0.0).floor() as i64;
                format!("-{}", fmt_time(rem.max(0) as f64))
            } else {
                fmt_time(dur)
            };
            let r = ui.add_sized(
                [right_w, 20.0],
                egui::Label::new(RichText::new(right).color(TEXT).size(12.5)).sense(Sense::click()),
            );
            if r.on_hover_text("Click to toggle between total and remaining time").clicked() {
                // remembered like MainWindow/ShowRemainingTime (libqt_plugin 0x1400ca6a0)
                self.show_remaining = !self.show_remaining;
                self.settings.show_remaining = self.show_remaining;
                self.save_settings();
            }
        });
    }

    fn control_bar(&mut self, ui: &mut Ui) {
        let acts = &mut self.actions;
        let p = &mut self.player;
        let playing = p.state == State::Playing;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let play_icon = if playing { Icon::Pause } else { Icon::Play };
            if icon_button(ui, play_icon, 34.0, false).on_hover_text(if playing { "Pause the playback" } else { "Play" }).clicked() {
                acts.push(Action::PlayPause);
            }
            ui.add_space(8.0);
            if icon_button(ui, Icon::Prev, 28.0, false).on_hover_text("Previous media in the playlist").clicked() {
                acts.push(Action::Prev);
            }
            if icon_button(ui, Icon::Stop, 28.0, false).on_hover_text("Stop playback").clicked() {
                acts.push(Action::Stop);
            }
            if icon_button(ui, Icon::Next, 28.0, false).on_hover_text("Next media in the playlist").clicked() {
                acts.push(Action::Next);
            }
            ui.add_space(8.0);
            if icon_button(ui, Icon::Fullscreen, 28.0, false).on_hover_text("Toggle the video in fullscreen").clicked() {
                acts.push(Action::Fullscreen);
            }
            if icon_button(ui, Icon::Extended, 28.0, self.ext_open).on_hover_text("Show extended settings").clicked() {
                acts.push(Action::Extended);
            }
            ui.add_space(8.0);
            if icon_button(ui, Icon::Playlist, 28.0, self.show_playlist).on_hover_text("Toggle playlist").clicked() {
                acts.push(Action::TogglePlaylist);
            }
            let loop_icon = match p.loop_mode {
                LoopMode::Off => Icon::LoopOff,
                LoopMode::All => Icon::LoopAll,
                LoopMode::One => Icon::LoopOne,
            };
            let tip = match p.loop_mode {
                LoopMode::Off => "Click to toggle between loop all, loop one and no loop",
                LoopMode::All => "Loop all",
                LoopMode::One => "Loop one",
            };
            if icon_button(ui, loop_icon, 28.0, p.loop_mode != LoopMode::Off).on_hover_text(tip).clicked() {
                acts.push(Action::Loop);
            }
            if icon_button(ui, Icon::Random, 28.0, p.random).on_hover_text("Random").clicked() {
                acts.push(Action::Random);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(4.0);
                let max = self.settings.slider_max;
                if let Some(v) = volume_wedge(ui, p.volume, max, p.muted) {
                    if p.muted {
                        p.set_muted(false);
                    }
                    p.set_volume(v, false);
                }
                let speaker = if p.muted || p.volume <= 0.0 { Icon::SpeakerMuted } else { Icon::Speaker };
                if icon_button(ui, speaker, 26.0, false).on_hover_text("Mute or unmute").clicked() {
                    acts.push(Action::Mute);
                }
            });
        });
    }

    /// The extra row from View > Advanced Controls: snapshot, A→B loop, frame by frame.
    fn advanced_bar(&mut self, ui: &mut Ui) {
        let acts = &mut self.actions;
        let ab_on = self.player.ab.0.is_some();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if icon_button(ui, Icon::Snapshot, 26.0, false).on_hover_text("Take a snapshot").clicked() {
                acts.push(Action::Snapshot);
            }
            let tip = match self.player.ab {
                (None, _) => "Loop from point A to point B continuously. Click to set point A",
                (Some(_), None) => "Click to set point B",
                _ => "Stop the A to B loop",
            };
            if icon_button(ui, Icon::AbLoop, 26.0, ab_on).on_hover_text(tip).clicked() {
                acts.push(Action::AbLoop);
            }
            if icon_button(ui, Icon::Frame, 26.0, false).on_hover_text("Frame by frame").clicked() {
                acts.push(Action::FrameStep);
            }
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let p = &self.player;
        ui.horizontal(|ui| {
            let state = match p.state {
                State::Playing => "Playing",
                State::Paused => "Paused",
                State::Stopped => "Stopped",
            };
            ui.label(RichText::new(state).size(12.0).color(TEXT));
            ui.separator();
            if let Some(t) = p.current_title() {
                ui.label(RichText::new(t).size(12.0).color(TEXT));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.2}x", p.rate)).size(12.0).color(TEXT));
                ui.separator();
                if p.is_active() {
                    ui.label(RichText::new(format!("{} / {}", fmt_time(p.position()), fmt_time(p.duration()))).size(12.0).color(TEXT));
                }
            });
        });
    }

    // ----- video area -----------------------------------------------------------------------

    fn video_area(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let rect = ui.max_rect();
        let resp = ui.interact(rect, ui.id().with("video"), Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::BLACK);

        let tex = self.player.texture(ctx);
        if let (Some(tex), true) = (tex, self.player.is_active()) {
            let aspect = self.player.display_aspect().max(0.01);
            let avail = rect.size();
            let mut size = vec2(avail.x, avail.x / aspect);
            if size.y > avail.y {
                size = vec2(avail.y * aspect, avail.y);
            }
            let r = Rect::from_center_size(rect.center(), size);
            painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            if let Some(text) = self.player.subtitle_text() {
                // sub-text-scale: 100 % = normal size
                let scale = self.player.sub_scale as f32 / mechanics::SUB_SCALE_NORMAL as f32;
                let size = (r.height() / 18.0).clamp(14.0, 42.0) * scale;
                outlined_text(&painter, pos2(r.center().x, r.bottom() - size * 1.2), Align2::CENTER_BOTTOM, &text, size);
            }
        } else if self.player.is_active() {
            // sound only, or the first picture isn't there yet
            let has_video = self.player.info.as_ref().map(|i| i.has_video).unwrap_or(false);
            if !has_video {
                audio_visual(&painter, rect, self.player.audio_level(), self.player.current_title().unwrap_or_default());
            }
        } else {
            let size = (rect.height().min(rect.width()) * 0.32).clamp(48.0, 220.0);
            icons::draw_logo(&painter, rect.center(), size);
        }

        // on-screen messages, top right like the original (vout_OSDMessage, 1 s)
        let mut osd_bottom = rect.top() + 12.0;
        if let Some((text, _)) = &self.player.osd {
            let size = (rect.height() / 14.0).clamp(16.0, 40.0);
            outlined_text(&painter, pos2(rect.right() - 16.0, rect.top() + 12.0), Align2::RIGHT_TOP, text, size);
            osd_bottom += size * 1.3;
        }
        // play / pause / mute icons, top right, fading out (vout_OSDIcon, 1.2 s)
        if let Some((icon, at)) = self.player.osd_icon {
            let age = at.elapsed().as_secs_f32();
            let total = mechanics::OSD_ICON_SECONDS as f32;
            let alpha = if age > total * 0.6 { ((total - age) / (total * 0.4)).clamp(0.0, 1.0) } else { 1.0 };
            let s = (rect.height() / 9.0).clamp(28.0, 64.0);
            let r = Rect::from_min_size(pos2(rect.right() - 16.0 - s, osd_bottom), vec2(s, s));
            let shadow = Color32::from_black_alpha((alpha * 200.0) as u8);
            let white = Color32::from_white_alpha((alpha * 255.0) as u8);
            let which = match icon {
                OsdIcon::Play => Icon::Play,
                OsdIcon::Pause => Icon::Pause,
                OsdIcon::Mute => Icon::SpeakerMuted,
            };
            icons::draw(&painter, r.translate(vec2(2.0, 2.0)), which, shadow);
            icons::draw(&painter, r, which, white);
        }
        // snapshot preview: top-left, a quarter of the picture's size, 4 s (VoutSaveSnapshot)
        if let Some((tex, at)) = &self.snap_preview {
            if at.elapsed().as_secs_f64() < mechanics::SNAPSHOT_PREVIEW_SECONDS {
                let aspect = self.player.display_aspect().max(0.01);
                let w = (rect.width().min(rect.height() * aspect)) * mechanics::SNAPSHOT_PREVIEW_SCALE;
                let pr = Rect::from_min_size(rect.min + vec2(12.0, 12.0), vec2(w, w / aspect));
                painter.image(tex.id(), pr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                painter.rect_stroke(pr, 0.0, Stroke::new(1.0_f32, Color32::from_gray(200)), StrokeKind::Outside);
                ctx.request_repaint();
            } else {
                self.snap_preview = None;
            }
        }

        // drop hint
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering {
            painter.rect_stroke(rect.shrink(6.0), 6.0, Stroke::new(3.0_f32, ACCENT), StrokeKind::Inside);
            outlined_text(&painter, rect.center(), Align2::CENTER_CENTER, "Drop to play", 26.0);
        }

        if resp.double_clicked() {
            self.actions.push(Action::Fullscreen);
        }
        if resp.hovered() {
            // the wheel goes through the hotkeys (hotkeys-y-wheel-mode = volume), so it shows the volume
            // Ctrl+wheel scales the subtitle text (key-subtitle-text-scale-up/down)
            let (scroll, ctrl) = ctx.input(|i| (i.raw_scroll_delta.y, i.modifiers.ctrl || i.modifiers.command));
            let dir = if scroll > 0.5 { 1 } else if scroll < -0.5 { -1 } else { 0 };
            if dir != 0 {
                self.key_actions.push(if ctrl { Action::SubtitleScale(dir) } else { Action::VolumeSteps(dir) });
            }
        }
        let acts = &mut self.actions;
        let playing = self.player.state == State::Playing;
        resp.context_menu(|ui| {
            item(ui, acts, if playing { "Pause" } else { "Play" }, "Space", Action::PlayPause);
            item(ui, acts, "Stop", "S", Action::Stop);
            item(ui, acts, "Previous", "P", Action::Prev);
            item(ui, acts, "Next", "N", Action::Next);
            ui.separator();
            item(ui, acts, "Fullscreen", "F", Action::Fullscreen);
            item(ui, acts, "Take Snapshot", "Shift+S", Action::Snapshot);
            item(ui, acts, "Playlist", "Ctrl+L", Action::TogglePlaylist);
            ui.separator();
            item(ui, acts, "Open File...", "Ctrl+O", Action::OpenFile);
            item(ui, acts, "Media Information", "Ctrl+I", Action::MediaInfo);
        });
    }

    fn playlist_view(&mut self, ui: &mut Ui) {
        let acts = &mut self.actions;
        let p = &self.player;
        egui::Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Playlist").strong().size(15.0).color(TEXT));
                ui.label(RichText::new(format!("({} items)", p.playlist.len())).color(Color32::GRAY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    egui::Frame::new()
                        .stroke(Stroke::new(1.0_f32, Color32::from_gray(180)))
                        .corner_radius(CornerRadius::same(3))
                        .inner_margin(egui::Margin::symmetric(4, 1))
                        .show(ui, |ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("🔍 Search the playlist").frame(false).desired_width(180.0));
                        });
                });
            });
            ui.separator();
            let bottom_h = 32.0;
            let list_h = (ui.available_height() - bottom_h).max(40.0);
            egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
                let w = ui.available_width();
                // column header
                let (hr, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
                ui.painter().rect_filled(hr, 0.0, Color32::from_rgb(243, 243, 243));
                ui.painter().line_segment([hr.left_bottom(), hr.right_bottom()], Stroke::new(1.0_f32, Color32::from_gray(210)));
                ui.painter().line_segment(
                    [pos2(hr.right() - 88.0, hr.top() + 3.0), pos2(hr.right() - 88.0, hr.bottom() - 3.0)],
                    Stroke::new(1.0_f32, Color32::from_gray(210)),
                );
                let hf = FontId::proportional(13.0);
                ui.painter().text(pos2(hr.left() + 6.0, hr.center().y), Align2::LEFT_CENTER, "Title", hf.clone(), TEXT);
                ui.painter().text(pos2(hr.right() - 80.0, hr.center().y), Align2::LEFT_CENTER, "Duration", hf, TEXT);
                let needle = self.search.to_lowercase();
                for (i, it) in p.playlist.iter().enumerate() {
                    if !needle.is_empty() && !it.title.to_lowercase().contains(&needle) {
                        continue;
                    }
                    let current = p.current == Some(i);
                    let selected = self.selected == Some(i);
                    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::click());
                    let bg = if selected {
                        Color32::from_rgb(204, 232, 255)
                    } else if resp.hovered() {
                        Color32::from_rgb(229, 243, 255)
                    } else if i % 2 == 1 {
                        Color32::from_rgb(247, 247, 247)
                    } else {
                        Color32::WHITE
                    };
                    ui.painter().rect_filled(rect, 0.0, bg);
                    let font = FontId::proportional(13.0);
                    let col = if current { ACCENT } else { TEXT };
                    let mut title = it.title.clone();
                    if current {
                        title = format!("▶ {}", title);
                    }
                    let clip = ui.painter().with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.right() - 90.0, rect.bottom())));
                    clip.text(pos2(rect.left() + 6.0, rect.center().y), Align2::LEFT_CENTER, title, font.clone(), col);
                    let d = it.duration.map(fmt_time).unwrap_or_else(|| "--:--".into());
                    ui.painter().text(pos2(rect.right() - 80.0, rect.center().y), Align2::LEFT_CENTER, d, font, col);
                    if resp.clicked() {
                        self.selected = Some(i);
                    }
                    if resp.double_clicked() {
                        acts.push(Action::PlayIndex(i));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Play").clicked() {
                            acts.push(Action::PlayIndex(i));
                        }
                        if ui.button("Remove Selected").clicked() {
                            acts.push(Action::RemoveIndex(i));
                        }
                        if ui.button("Information...").clicked() {
                            acts.push(Action::PlayIndex(i));
                            acts.push(Action::MediaInfo);
                        }
                    });
                }
                if p.playlist.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Drop files here, or use Media > Open File").color(Color32::GRAY).size(14.0));
                    });
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("➕ Add files...").clicked() {
                    acts.push(Action::AddToPlaylist);
                }
                if ui.add_enabled(self.selected.is_some(), egui::Button::new("Remove")).clicked() {
                    if let Some(i) = self.selected {
                        acts.push(Action::RemoveIndex(i));
                    }
                }
                if ui.add_enabled(!p.playlist.is_empty(), egui::Button::new("Clear")).clicked() {
                    acts.push(Action::ClearPlaylist);
                }
                if ui.button("Save...").clicked() {
                    acts.push(Action::SavePlaylist);
                }
            });
        });
        if !ui.ctx().wants_keyboard_input() && ui.ctx().input(|i| i.key_pressed(Key::Delete)) {
            if let Some(i) = self.selected {
                self.actions.push(Action::RemoveIndex(i));
            }
        }
        if !ui.ctx().wants_keyboard_input() && ui.ctx().input(|i| i.key_pressed(Key::Enter)) {
            if let Some(i) = self.selected {
                self.actions.push(Action::PlayIndex(i));
            }
        }
    }

    // ----- dialogs --------------------------------------------------------------------------

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.net_open {
            let mut open = true;
            let mut go = false;
            egui::Window::new("Open Media")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Please enter a network URL:");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.net_url).hint_text("https://www.example.com/video.mp4").desired_width(380.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        go = true;
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Play").clicked() {
                            go = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.net_open = false;
                        }
                    });
                });
            if go && !self.net_url.trim().is_empty() {
                let url = self.net_url.trim().to_string();
                let i = self.player.add_location(&url);
                self.player.play_index(i, ctx);
                self.net_open = false;
            }
            if !open {
                self.net_open = false;
            }
        }

        if self.goto_open {
            let mut open = true;
            let mut go = false;
            egui::Window::new("Go to Time")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Go to time (H:MM:SS):");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.goto_text).desired_width(160.0));
                    r.request_focus();
                    if ui.input(|i| i.key_pressed(Key::Enter)) {
                        go = true;
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Go").clicked() {
                            go = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.goto_open = false;
                        }
                    });
                });
            if go {
                if let Some(t) = parse_time(&self.goto_text) {
                    self.player.seek(t);
                }
                self.goto_open = false;
            }
            if !open {
                self.goto_open = false;
            }
        }

        if self.info_open {
            let mut open = true;
            let p = &self.player;
            egui::Window::new("Current Media Information").open(&mut open).default_width(380.0).show(ctx, |ui| {
                match (&p.info, p.current_title()) {
                    (Some(info), Some(title)) => {
                        let loc = p.current.and_then(|i| p.playlist.get(i)).map(|i| i.location.clone()).unwrap_or_default();
                        egui::Grid::new("info").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                            ui.label(RichText::new("Title").strong());
                            ui.label(title);
                            ui.end_row();
                            ui.label(RichText::new("Location").strong());
                            ui.label(loc);
                            ui.end_row();
                            ui.label(RichText::new("Duration").strong());
                            ui.label(fmt_time(info.duration));
                            ui.end_row();
                            if info.file_size > 0 {
                                ui.label(RichText::new("Size").strong());
                                ui.label(format!("{:.1} MB", info.file_size as f64 / 1_048_576.0));
                                ui.end_row();
                            }
                            if info.has_video {
                                ui.label(RichText::new("Video codec").strong());
                                ui.label(&info.video_codec);
                                ui.end_row();
                                ui.label(RichText::new("Video resolution").strong());
                                ui.label(format!("{}x{}", info.width, info.height));
                                ui.end_row();
                                if info.fps > 0.0 {
                                    ui.label(RichText::new("Frame rate").strong());
                                    ui.label(format!("{:.3}", info.fps));
                                    ui.end_row();
                                }
                            }
                            if info.has_audio {
                                ui.label(RichText::new("Audio codec").strong());
                                ui.label(&info.audio_codec);
                                ui.end_row();
                                ui.label(RichText::new("Channels").strong());
                                ui.label(match info.audio_channels {
                                    1 => "Mono".to_string(),
                                    2 => "Stereo".to_string(),
                                    6 => "5.1".to_string(),
                                    8 => "7.1".to_string(),
                                    n => format!("{}", n),
                                });
                                ui.end_row();
                                ui.label(RichText::new("Sample rate").strong());
                                ui.label(format!("{} Hz", info.audio_rate));
                                ui.end_row();
                            }
                        });
                    }
                    _ => {
                        ui.label("Nothing is playing.");
                    }
                }
            });
            if !open {
                self.info_open = false;
            }
        }

        if self.prefs_open {
            let mut open = true;
            egui::Window::new("Simple Preferences").open(&mut open).default_width(360.0).show(ctx, |ui| {
                ui.heading("Interface");
                ui.horizontal(|ui| {
                    ui.label("Maximum volume displayed (%):");
                    let mut v = (self.settings.slider_max * 100.0).round() as i32;
                    if ui.add(egui::DragValue::new(&mut v).range(60..=300)).changed() {
                        self.settings.slider_max = v as f32 / 100.0;
                    }
                });
                ui.checkbox(&mut self.settings.remember_recent, "Save recently played items");
                ui.checkbox(&mut self.settings.pause_minimized, "Pause playback when minimized");
                ui.add_space(8.0);
                ui.heading("Hotkeys: jump sizes (seconds)");
                let names = ["Very short (Shift+Arrow)", "Short (Alt+Arrow)", "Medium (Ctrl+Arrow)", "Long (Ctrl+Alt+Arrow)"];
                for (i, n) in names.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(*n);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add(egui::DragValue::new(&mut self.player.jump_sizes[i]).range(1.0..=3600.0).speed(1.0));
                        });
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Reset Preferences").clicked() {
                        let d = Settings::default();
                        self.settings.slider_max = d.slider_max;
                        self.player.jump_sizes = d.jump_sizes;
                    }
                    if ui.button("Save").clicked() {
                        self.prefs_open = false;
                        self.save_settings();
                    }
                });
            });
            if !open {
                self.prefs_open = false;
                self.save_settings();
            }
        }

        if self.ext_open {
            let mut open = true;
            egui::Window::new("Adjustments and Effects").open(&mut open).default_width(320.0).show(ctx, |ui| {
                let p = &mut self.player;
                ui.heading("Playback speed");
                ui.horizontal(|ui| {
                    let mut r = p.rate;
                    let resp = ui.add(egui::Slider::new(&mut r, 0.25..=4.0).logarithmic(true).suffix("x").fixed_decimals(2));
                    if resp.changed() {
                        p.set_rate(r);
                    }
                    if ui.button("1.00x").clicked() {
                        p.normal_rate();
                    }
                });
                ui.add_space(6.0);
                ui.heading("Volume");
                let mut v = p.volume * 100.0;
                if ui.add(egui::Slider::new(&mut v, 0.0..=200.0).suffix(" %").step_by(5.0)).changed() {
                    p.set_volume(v / 100.0, false);
                }
                ui.add_space(6.0);
                ui.heading("Subtitle delay");
                match &mut p.subtitles {
                    Some(s) => {
                        let mut ms = (s.delay * 1000.0) as i64;
                        if ui.add(egui::DragValue::new(&mut ms).suffix(" ms").speed(10.0)).changed() {
                            s.delay = ms as f64 / 1000.0;
                        }
                    }
                    None => {
                        ui.label(RichText::new("No subtitles loaded").color(Color32::GRAY));
                    }
                }
            });
            if !open {
                self.ext_open = false;
            }
        }

        if self.keys_open {
            let mut open = true;
            egui::Window::new("Keyboard Shortcuts").open(&mut open).default_width(340.0).show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    egui::Grid::new("keys").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                        for (k, what) in SHORTCUTS {
                            ui.label(RichText::new(*k).monospace());
                            ui.label(*what);
                            ui.end_row();
                        }
                    });
                });
            });
            if !open {
                self.keys_open = false;
            }
        }

        if self.about_open {
            let mut open = true;
            egui::Window::new(format!("About {}", APP_NAME))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(96.0, 96.0), Sense::hover());
                        icons::draw_logo(ui.painter(), r.center(), 70.0);
                        ui.heading(APP_NAME);
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        ui.add_space(6.0);
                        ui.label("A simple player for your videos and music.");
                        ui.label("Pictures and sound are decoded by Windows' own media decoders.");
                    });
                });
            if !open {
                self.about_open = false;
            }
        }

        if let Some(err) = self.player.error.clone() {
            let mut open = true;
            let mut ok = false;
            egui::Window::new("Errors")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(err);
                    ui.add_space(6.0);
                    if ui.button("Close").clicked() {
                        ok = true;
                    }
                });
            if !open || ok {
                self.player.error = None;
            }
        }
    }

    fn fullscreen_controls(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let w = (screen.width() * 0.6).clamp(420.0, 900.0);
        egui::Area::new(egui::Id::new("fs-controller"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -24.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(240, 240, 240, 235))
                    .corner_radius(CornerRadius::same(6))
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(170)))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_width(w);
                        self.time_bar(ui);
                        self.control_bar(ui);
                    });
            });
    }
}

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // scripted test actions
        if !self.test.keys.is_empty() {
            let n = self.frames_drawn + 1;
            for part in self.test.keys.clone().split(',') {
                let Some((f, act)) = part.split_once(':') else { continue };
                if f.trim().parse::<u32>().ok() != Some(n) {
                    continue;
                }
                let act = act.trim();
                let a = match act {
                    "pause" | "play" => Some(Action::PlayPause),
                    "stop" => Some(Action::Stop),
                    "next" => Some(Action::Next),
                    "prev" => Some(Action::Prev),
                    "faster" => Some(Action::Faster),
                    "normal" => Some(Action::NormalSpeed),
                    "slower" => Some(Action::Slower),
                    "finer" => Some(Action::FasterFine),
                    "loop" => Some(Action::Loop),
                    "random" => Some(Action::Random),
                    "mute" => Some(Action::Mute),
                    "volup" => Some(Action::VolumeSteps(1)),
                    "voldown" => Some(Action::VolumeSteps(-1)),
                    "playlist" => Some(Action::TogglePlaylist),
                    "aspect" => Some(Action::Aspect(None)),
                    "position" => Some(Action::ShowPosition),
                    "frame" => Some(Action::FrameStep),
                    "jump" => Some(Action::Jump(2.0)),
                    "back" => Some(Action::Jump(-2.0)),
                    "info" => Some(Action::MediaInfo),
                    "advanced" => Some(Action::ToggleAdvanced),
                    "ab" => Some(Action::AbLoop),
                    "keys" => Some(Action::Shortcuts),
                    _ => None,
                };
                if let Some(t) = act.strip_prefix("seek=").and_then(|s| s.parse::<f64>().ok()) {
                    self.player.seek(t);
                } else if let Some(a) = a {
                    // scripted test steps act like key presses
                    self.key_actions.push(a);
                }
            }
            if std::env::var_os("LVP_DEBUG").is_some() && n % 30 == 0 {
                eprintln!("{}", self.player.debug_line());
            }
        }

        // a check run: draw ~120 frames, take a picture, quit
        let shot_at = self.test.frames.unwrap_or(120);
        if !self.test.keys.is_empty() && self.screenshot.is_none() {
            self.frames_drawn += 1;
        }
        if let Some(path) = self.screenshot.clone() {
            self.frames_drawn += 1;
            ctx.request_repaint();
            if self.frames_drawn >= shot_at && !self.screenshot_requested {
                self.screenshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                let _ = save_png(&path, &img);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.frames_drawn > shot_at + 600 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // files dropped onto the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            if dropped.len() == 1 && dropped[0].extension().map(|e| e.eq_ignore_ascii_case("srt")).unwrap_or(false) {
                if let Some(s) = Subtitles::load(&dropped[0]) {
                    self.player.show_osd(format!("Subtitle track: {}", s.name));
                    self.player.subtitles = Some(s);
                }
            } else {
                self.player.open_and_play(&dropped, ctx);
            }
        }

        // pause when minimized (preference)
        if self.settings.pause_minimized && ctx.input(|i| i.viewport().minimized.unwrap_or(false)) && self.player.state == State::Playing {
            self.player.pause();
        }

        self.keyboard(ctx);
        self.player.update(ctx);

        // window title like "<title> - Lab Video Player"
        let title = match self.player.current_title() {
            Some(t) if self.player.is_active() => format!("{} - {}", t, APP_NAME),
            _ => APP_NAME.to_string(),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        // mouse movement shows the controls in fullscreen
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        if pointer != self.last_pointer {
            self.last_pointer = pointer;
            self.last_mouse_move = Instant::now();
        }

        let chrome = !self.fullscreen;
        if chrome && !self.minimal {
            egui::TopBottomPanel::top("menu")
                .frame(egui::Frame::new().fill(MENU_BG).inner_margin(egui::Margin::symmetric(4, 1)))
                .show_separator_line(false)
                .show(ctx, |ui| self.menu_bar(ui));
        }
        if chrome && self.settings.status_bar {
            egui::TopBottomPanel::bottom("status")
                .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(8, 2)))
                .show(ctx, |ui| self.status_bar(ui));
        }
        if chrome {
            egui::TopBottomPanel::bottom("controls")
                .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 6, right: 6, top: 4, bottom: 4 }))
                .show_separator_line(true)
                .show(ctx, |ui| {
                    if self.advanced {
                        self.advanced_bar(ui);
                    }
                    self.time_bar(ui);
                    ui.add_space(2.0);
                    self.control_bar(ui);
                });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::BLACK)).show(ctx, |ui| {
            if self.show_playlist && !self.fullscreen {
                self.playlist_view(ui);
            } else {
                self.video_area(ui, ctx);
            }
        });
        if self.fullscreen && self.last_mouse_move.elapsed().as_secs_f32() < 2.0 {
            self.fullscreen_controls(ctx);
        }

        self.dialogs(ctx);

        // run what the person asked for: keys and the wheel show on-screen feedback (hotkeys
        // plugin), menus and buttons don't (they act on the player directly)
        let keys = std::mem::take(&mut self.key_actions);
        for a in keys {
            self.apply(a, ctx);
        }
        let actions = std::mem::take(&mut self.actions);
        for a in actions {
            let keeps_message = matches!(a, Action::Snapshot | Action::AddSubtitle | Action::ShowPosition);
            let (osd, icon) = (self.player.osd.clone(), self.player.osd_icon);
            self.apply(a, ctx);
            if !keeps_message {
                self.player.osd = osd;
                self.player.osd_icon = icon;
            }
        }
        if self.player.want_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_settings();
        }

        // keep drawing while playing; check now and then otherwise
        if self.player.state == State::Playing || self.player.osd.is_some() || self.fullscreen {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

impl Drop for PlayerApp {
    fn drop(&mut self) {
        self.save_settings();
    }
}

// ----- small widgets ------------------------------------------------------------------------

fn item(ui: &mut Ui, acts: &mut Vec<Action>, label: &str, shortcut: &str, action: Action) {
    let mut b = egui::Button::new(label);
    if !shortcut.is_empty() {
        b = b.shortcut_text(shortcut);
    }
    if ui.add(b).clicked() {
        acts.push(action);
    }
}

/// Shorten long text in the middle ("C:\Users\...\film.mp4").
fn elide_middle(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let head = keep / 2;
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(chars[chars.len() - tail..].iter());
    out
}

fn media_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new()
        .set_title("Select one or more files to open")
        .add_filter("Media Files", crate::player::MEDIA_EXTENSIONS)
        .add_filter("Video Files", &["mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "mpg", "mpeg", "ts", "m2ts", "3gp", "flv"])
        .add_filter("Audio Files", &["mp3", "m4a", "aac", "wav", "flac", "wma", "ogg", "opus", "aiff", "ac3", "mka"])
        .add_filter("All Files", &["*"])
}

fn icon_button(ui: &mut Ui, icon: Icon, size: f32, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let p = ui.painter();
    if resp.is_pointer_button_down_on() {
        p.rect_filled(rect, 3.0, Color32::from_rgb(204, 228, 247));
        p.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, Color32::from_rgb(0, 84, 153)), StrokeKind::Inside);
    } else if resp.hovered() {
        p.rect_filled(rect, 3.0, HOVER_BG);
        p.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, HOVER_BORDER), StrokeKind::Inside);
    }
    let color = if active { ACCENT } else { ICON };
    let inner = rect.shrink(size * if size > 30.0 { 0.16 } else { 0.2 });
    icons::draw(p, inner, icon, color);
    resp
}

enum SeekEvent {
    Drag(f64),
    Release(f64),
}

fn seek_slider(ui: &mut Ui, width: f32, frac: f64, enabled: bool, duration: f64) -> Option<SeekEvent> {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 20.0), if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let p = ui.painter();
    let groove = Rect::from_min_max(pos2(rect.left(), rect.center().y - 4.0), pos2(rect.right(), rect.center().y + 4.0));
    p.rect_filled(groove, 4.0, Color32::from_rgb(222, 222, 222));
    p.rect_stroke(groove, 4.0, Stroke::new(1.0_f32, Color32::from_rgb(170, 170, 170)), StrokeKind::Inside);
    let x = groove.left() + groove.width() * frac as f32;
    if enabled && frac > 0.0 {
        let filled = Rect::from_min_max(groove.min, pos2(x.max(groove.left() + 4.0), groove.max.y));
        // two-tone blue fill
        p.rect_filled(filled, 4.0, Color32::from_rgb(64, 140, 230));
        let shine = Rect::from_min_max(filled.min, pos2(filled.max.x, filled.center().y));
        p.rect_filled(shine.shrink2(vec2(1.0, 1.0)), CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, Color32::from_rgb(110, 172, 242));
    }
    if enabled {
        let hovered = resp.hovered() || resp.dragged();
        let r = if hovered { 7.0 } else { 5.5 };
        p.circle_filled(pos2(x, groove.center().y), r, Color32::WHITE);
        p.circle_stroke(pos2(x, groove.center().y), r, Stroke::new(1.0_f32, Color32::from_rgb(120, 120, 120)));
        if let Some(hp) = resp.hover_pos() {
            let f = ((hp.x - groove.left()) / groove.width()).clamp(0.0, 1.0) as f64;
            resp.clone().on_hover_text_at_pointer(fmt_time(f * duration));
        }
    }
    let frac_at = |pos: egui::Pos2| ((pos.x - groove.left()) / groove.width()).clamp(0.0, 1.0) as f64;
    if resp.drag_stopped() {
        if let Some(pos) = resp.interact_pointer_pos() {
            return Some(SeekEvent::Release(frac_at(pos)));
        }
    }
    if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            return Some(SeekEvent::Drag(frac_at(pos)));
        }
    }
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            return Some(SeekEvent::Release(frac_at(pos)));
        }
    }
    None
}

/// The wedge-shaped volume slider (0 .. `max`), coloured from green to red.
fn volume_wedge(ui: &mut Ui, volume: f32, max: f32, muted: bool) -> Option<f32> {
    let (rect, resp) = ui.allocate_exact_size(vec2(86.0, 26.0), Sense::click_and_drag());
    let p = ui.painter();
    let wedge = Rect::from_min_max(pos2(rect.left() + 2.0, rect.top() + 4.0), pos2(rect.right() - 2.0, rect.bottom() - 3.0));
    let (l, r, t, b) = (wedge.left(), wedge.right(), wedge.top(), wedge.bottom());
    let top_at = |x: f32| b - (b - t) * ((x - l) / (r - l));
    // empty wedge
    p.add(Shape::convex_polygon(vec![pos2(l, b), pos2(r, b), pos2(r, t)], Color32::from_rgb(214, 214, 214), Stroke::NONE));
    // filled part as thin coloured strips
    let frac = (volume / max).clamp(0.0, 1.0);
    let fill_x = l + (r - l) * frac;
    let strips = 40;
    for i in 0..strips {
        let x0 = l + (r - l) * i as f32 / strips as f32;
        let x1 = l + (r - l) * (i + 1) as f32 / strips as f32;
        if x0 >= fill_x {
            break;
        }
        let x1 = x1.min(fill_x);
        let col = gradient_at((x0 - l) / (r - l), max);
        let col = if muted { Color32::from_gray(((col.r() as u32 + col.g() as u32 + col.b() as u32) / 3) as u8) } else { col };
        p.add(Shape::convex_polygon(vec![pos2(x0, b), pos2(x1, b), pos2(x1, top_at(x1)), pos2(x0, top_at(x0))], col, Stroke::NONE));
    }
    p.add(Shape::closed_line(vec![pos2(l, b), pos2(r, b), pos2(r, t)], Stroke::new(1.0_f32, Color32::from_rgb(150, 150, 150))));
    let text = format!("{}%", (volume * 100.0).round() as i32);
    p.text(pos2(l + 1.0, t - 1.0), Align2::LEFT_TOP, text, FontId::proportional(11.0), Color32::from_rgb(60, 60, 60));
    let mut out = None;
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let f = ((pos.x - l) / (r - l)).clamp(0.0, 1.0);
            // stay on the 5 % grid like the keyboard does
            let v = (f * max / mechanics::VOLUME_STEP * 4.0).round() / 4.0 * mechanics::VOLUME_STEP;
            out = Some(v.clamp(0.0, max));
        }
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll.abs() > 0.5 {
            let steps = if scroll > 0.0 { 1 } else { -1 };
            out = Some(mechanics::volume_after_steps(volume, steps).min(max));
        }
    }
    let resp = resp.on_hover_text(format!("Volume {}%", (volume * 100.0).round() as i32));
    let _ = resp;
    out
}

/// Colour of the volume wedge at a fraction of its width. The four colours sit at
/// 0 %, 50 %, 100 % and the slider maximum.
fn gradient_at(f: f32, max: f32) -> Color32 {
    let c = mechanics::SLIDER_COLOURS.map(|c| Color32::from_rgb(c[0], c[1], c[2]));
    let v = f * max; // volume at this point, 1.0 = 100 %
    let stops = [0.0, 0.5, 1.0, max.max(1.01)];
    for i in 0..3 {
        if v <= stops[i + 1] {
            let t = (v - stops[i]) / (stops[i + 1] - stops[i]);
            return icons::lerp_color(c[i], c[i + 1], t);
        }
    }
    c[3]
}

fn outlined_text(p: &egui::Painter, pos: egui::Pos2, align: Align2, text: &str, size: f32) {
    let font = FontId::proportional(size);
    for (dx, dy) in [(-1.5, 0.0), (1.5, 0.0), (0.0, -1.5), (0.0, 1.5), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
        p.text(pos + vec2(dx, dy), align, text, font.clone(), Color32::BLACK);
    }
    p.text(pos, align, text, font, Color32::WHITE);
}

/// What the video area shows while playing music: the title and a level bar.
fn audio_visual(p: &egui::Painter, rect: Rect, level: f32, title: String) {
    let c = rect.center();
    let size = (rect.height().min(rect.width()) * 0.28).clamp(48.0, 180.0);
    icons::draw_logo(p, c - vec2(0.0, size * 0.25), size);
    p.text(
        pos2(c.x, c.y + size * 0.55),
        Align2::CENTER_TOP,
        title,
        FontId::proportional(18.0),
        Color32::from_gray(220),
    );
    let w = size * 1.6;
    let bar = Rect::from_min_size(pos2(c.x - w / 2.0, c.y + size * 0.55 + 30.0), vec2(w, 6.0));
    p.rect_filled(bar, 3.0, Color32::from_gray(50));
    let lvl = Rect::from_min_size(bar.min, vec2(w * level.clamp(0.0, 1.0), 6.0));
    p.rect_filled(lvl, 3.0, Color32::from_rgb(255, 150, 50));
}

fn setup_style(ctx: &egui::Context) {
    // Ctrl+0 / Ctrl+wheel belong to the subtitle text size, not to zooming the window
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    let mut v = egui::Visuals::light();
    v.panel_fill = BG;
    v.window_fill = Color32::WHITE;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = Color32::from_rgb(204, 232, 255);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.button_padding = vec2(8.0, 3.0);
        s.spacing.menu_margin = egui::Margin::same(4);
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.5));
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.5));
    });
}

fn save_png(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let file = std::fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut enc = png::Encoder::new(w, img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    let mut data = Vec::with_capacity(img.pixels.len() * 4);
    for px in &img.pixels {
        let [r, g, b, a] = px.to_srgba_unmultiplied();
        data.extend_from_slice(&[r, g, b, a]);
    }
    writer.write_image_data(&data).map_err(std::io::Error::other)?;
    Ok(())
}

/// Local date and time as "YYYY-MM-DD-HHhMMmSSs" plus milliseconds (3 digits), the snapshot
/// stamp of vout_snapshot_SaveImage (libvlccore 0x1400722a0).
fn snapshot_stamp() -> String {
    #[cfg(windows)]
    {
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        format!(
            "{:04}-{:02}-{:02}-{:02}h{:02}m{:02}s{:03}",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
        )
    }
    #[cfg(not(windows))]
    {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("{ms}")
    }
}

/// Save the picture on screen into the Pictures folder (config_GetUserDir(0x86), 0x140072290).
/// Returns the saved file's full path.
fn save_snapshot(p: &Player) -> Result<String, String> {
    let Some(frame) = &p.frame else { return Err("No picture to save".into()) };
    let dir = std::env::var_os("USERPROFILE")
        .map(|h| PathBuf::from(h).join("Pictures"))
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let name = format!("{}{}.{}", mechanics::SNAPSHOT_PREFIX, snapshot_stamp(), mechanics::SNAPSHOT_FORMAT);
    let path = dir.join(&name);
    match save_png(&path, frame) {
        Ok(()) => Ok(path.to_string_lossy().to_string()),
        Err(e) => Err(format!("Snapshot failed: {}", e)),
    }
}

const SHORTCUTS: &[(&str, &str)] = &[
    ("Space", "Play / Pause"),
    ("S", "Stop"),
    ("N / P", "Next / Previous in the playlist"),
    ("F, double-click", "Fullscreen"),
    ("Esc", "Leave fullscreen"),
    ("Shift+Left/Right", "Jump 3 seconds"),
    ("Alt+Left/Right", "Jump 10 seconds"),
    ("Ctrl+Left/Right", "Jump 1 minute"),
    ("Ctrl+Alt+Left/Right", "Jump 5 minutes"),
    ("Ctrl+T", "Jump to a specific time"),
    ("T", "Show the position"),
    ("E", "Next frame"),
    ("+ / -", "Faster / Slower"),
    ("] / [", "Faster / Slower (fine)"),
    ("=", "Normal speed"),
    ("Ctrl+Up/Down, wheel", "Volume up / down (5 %)"),
    ("M", "Mute"),
    ("L", "Loop: off / all / one"),
    ("R", "Random"),
    ("A", "Aspect ratio"),
    ("V", "Subtitles on / off"),
    ("G / H", "Subtitle delay -/+ 50 ms"),
    ("Ctrl+wheel / Ctrl+0", "Subtitle text size / normal size"),
    ("Ctrl+1 … Ctrl+9", "Open a recent item"),
    ("Shift+S", "Snapshot"),
    ("Ctrl+L", "Playlist"),
    ("Ctrl+H", "Minimal interface"),
    ("Ctrl+O", "Open file"),
    ("Ctrl+N", "Open network stream"),
    ("Ctrl+I", "Media information"),
    ("Ctrl+Q", "Quit"),
];
