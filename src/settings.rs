//! Remembered settings, stored as plain "key=value" lines in the user's app-data folder.

use std::path::PathBuf;

use crate::mechanics;

#[derive(Clone, Debug)]
pub struct Settings {
    pub volume: f32,
    pub slider_max: f32,
    pub jump_sizes: [f64; 4],
    pub recent: Vec<String>,
    pub loop_mode: u8,
    pub random: bool,
    pub status_bar: bool,
    pub remember_recent: bool,
    pub pause_minimized: bool,
    pub show_remaining: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            slider_max: mechanics::SLIDER_VOLUME_MAX,
            jump_sizes: [
                mechanics::JUMP_EXTRASHORT_S,
                mechanics::JUMP_SHORT_S,
                mechanics::JUMP_MEDIUM_S,
                mechanics::JUMP_LONG_S,
            ],
            recent: Vec::new(),
            loop_mode: 0,
            random: false,
            status_bar: false,
            remember_recent: true,
            pause_minimized: false,
            show_remaining: false,
        }
    }
}

fn file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(PathBuf::from))?;
    Some(base.join("LabVideoPlayer").join("settings.txt"))
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Settings::default();
        let Some(path) = file() else { return s };
        let Ok(text) = std::fs::read_to_string(path) else { return s };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "volume" => s.volume = v.parse().unwrap_or(1.0f32).clamp(0.0, mechanics::VOLUME_MAX),
                "slider_max" => s.slider_max = v.parse().unwrap_or(1.25f32).clamp(0.6, 3.0),
                "jump_extrashort" => s.jump_sizes[0] = v.parse().unwrap_or(s.jump_sizes[0]),
                "jump_short" => s.jump_sizes[1] = v.parse().unwrap_or(s.jump_sizes[1]),
                "jump_medium" => s.jump_sizes[2] = v.parse().unwrap_or(s.jump_sizes[2]),
                "jump_long" => s.jump_sizes[3] = v.parse().unwrap_or(s.jump_sizes[3]),
                "recent" => {
                    if !v.is_empty() {
                        s.recent.push(v.to_string())
                    }
                }
                "loop" => s.loop_mode = v.parse().unwrap_or(0),
                "random" => s.random = v == "1",
                "status_bar" => s.status_bar = v == "1",
                "remember_recent" => s.remember_recent = v != "0",
                "pause_minimized" => s.pause_minimized = v == "1",
                "show_remaining" => s.show_remaining = v == "1",
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let Some(path) = file() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut out = String::new();
        out += &format!("volume={}\n", self.volume);
        out += &format!("slider_max={}\n", self.slider_max);
        out += &format!("jump_extrashort={}\n", self.jump_sizes[0]);
        out += &format!("jump_short={}\n", self.jump_sizes[1]);
        out += &format!("jump_medium={}\n", self.jump_sizes[2]);
        out += &format!("jump_long={}\n", self.jump_sizes[3]);
        out += &format!("loop={}\n", self.loop_mode);
        out += &format!("random={}\n", self.random as u8);
        out += &format!("status_bar={}\n", self.status_bar as u8);
        out += &format!("remember_recent={}\n", self.remember_recent as u8);
        out += &format!("pause_minimized={}\n", self.pause_minimized as u8);
        out += &format!("show_remaining={}\n", self.show_remaining as u8);
        if self.remember_recent {
            for r in &self.recent {
                out += &format!("recent={}\n", r);
            }
        }
        let _ = std::fs::write(path, out);
    }
}
