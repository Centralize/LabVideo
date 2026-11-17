//! The player's rules and numbers, as written in MECHANICS.md / mechanics.json.

/// Jump sizes in seconds.
pub const JUMP_EXTRASHORT_S: f64 = 3.0;
pub const JUMP_SHORT_S: f64 = 10.0;
pub const JUMP_MEDIUM_S: f64 = 60.0;
pub const JUMP_LONG_S: f64 = 300.0;

/// Volume: one step is 12.8 / 256 = 5 %.
pub const VOLUME_STEP: f32 = 12.8 / 256.0;
/// Keyboard volume can reach 200 %.
pub const VOLUME_MAX: f32 = 2.0;
/// The slider in the window reaches 125 %.
pub const SLIDER_VOLUME_MAX: f32 = 1.25;
/// Slider colours (low → high).
pub const SLIDER_COLOURS: [[u8; 3]; 4] = [[153, 210, 153], [20, 210, 20], [255, 199, 15], [245, 39, 29]];

/// Speed table used by Faster / Slower.
pub const RATES: [f32; 17] = [
    1.0 / 64.0,
    1.0 / 32.0,
    1.0 / 16.0,
    1.0 / 8.0,
    1.0 / 4.0,
    1.0 / 3.0,
    1.0 / 2.0,
    2.0 / 3.0,
    1.0,
    3.0 / 2.0,
    2.0,
    3.0,
    4.0,
    8.0,
    16.0,
    32.0,
    64.0,
];
pub const FASTER_FACTOR: f32 = 1.1;
pub const SLOWER_FACTOR: f32 = 0.9;
pub const FINE_STEP: f64 = 0.1;
pub const FINE_MIN: f32 = 0.03125;
pub const FINE_MAX: f32 = 31.25;

/// Snapshots (vout_snapshot_SaveImage 0x1400722a0, VoutSaveSnapshot 0x14007fac0): saved in the
/// Pictures folder as <prefix><YYYY-MM-DD-HHhMMmSSs><milliseconds, 3 digits>.png; the original's
/// prefix is its own name, this player uses its own.
pub const SNAPSHOT_PREFIX: &str = "labsnap-";
pub const SNAPSHOT_FORMAT: &str = "png";
/// After saving, a preview at a quarter of the picture's size shows top-left for 4 s.
pub const SNAPSHOT_PREVIEW_SECONDS: f64 = 4.0;
pub const SNAPSHOT_PREVIEW_SCALE: f32 = 0.25;

/// Subtitle delay keys (G / H) move the delay by 50 ms (libhotkeys case 0x32/0x33: ±50000 µs).
pub const SUB_DELAY_STEP_S: f64 = 0.05;

/// Subtitle text size (libhotkeys cases 0x3e-0x40): 100 % is normal; the step is 10 within
/// 50-150 %, else 25; the result is rounded down to the step and kept within 25-500 %.
pub const SUB_SCALE_NORMAL: i32 = 100;
pub fn sub_scale_step(scale: i32, up: bool) -> i32 {
    let near = if scale <= 100 { 100 - scale < 50 } else { scale < 150 };
    let step = if near { 10 } else { 25 };
    let delta = if up { step } else { -step };
    let v = scale + delta;
    let v = v - v.rem_euclid(step);
    v.clamp(25, 500)
}

/// Recent media (RecentsMRL::addRecent 0x14002e120): newest first, an item played again moves
/// to the top, at most 30 are kept; the menu (0x1400069b0) shows the first 10, with Ctrl+1..Ctrl+9.
pub const RECENT_MAX: usize = 30;
pub const RECENT_MENU_MAX: usize = 10;

/// Surround to stereo (libsimple_channel_mixer FUN_140001770, constant 0x1400061b8):
/// left = L + 0.7071·(C + surround left), right = R + 0.7071·(C + surround right); LFE dropped.
pub const SURROUND_MIX: f32 = 0.7071;

/// How much of an AC-3 stream's own dynamic range compression is applied: the decoder's
/// "drc_scale" option (libavcodec_plugin.dll 0x1410a4020 / 0x1410a4c40) defaults to 1.0
/// (the stream's gain words in full) within 0–6. Gain = stream gain ^ scale.
pub const AC3_DRC_SCALE: f32 = 1.0;

/// On-screen messages show for 1 s at the top right (vout_OSDMessage: stop = start + 1000000 µs).
pub const OSD_SECONDS: f64 = 1.0;
/// On-screen icons (play, pause, mute) show for 1.2 s and fade out (vout_OSDIcon: start + 1200000 µs).
pub const OSD_ICON_SECONDS: f64 = 1.2;

/// Pitch-keeping at other speeds is on by default (audio-time-stretch = 1).
pub const KEEP_PITCH: bool = true;
/// Tempo scaler: stride 30 ms, overlap 20 % of the stride, search 14 ms.
pub const TEMPO_STRIDE_MS: f64 = 30.0;
pub const TEMPO_OVERLAP: f64 = 0.2;
pub const TEMPO_SEARCH_MS: f64 = 14.0;

/// Speeds the rebuild still plays sound at (estimated limit).
pub const AUDIBLE_RATE_MIN: f32 = 0.25;
pub const AUDIBLE_RATE_MAX: f32 = 4.0;

/// Volume after `steps` keyboard steps (aout_VolumeUpdate rule).
pub fn volume_after_steps(vol: f32, steps: i32) -> f32 {
    let v = (vol + steps as f32 * VOLUME_STEP).clamp(0.0, VOLUME_MAX);
    (v / VOLUME_STEP).round() * VOLUME_STEP
}

/// Next speed for Faster.
pub fn rate_faster(rate: f32) -> f32 {
    let r = rate * FASTER_FACTOR;
    for &t in RATES.iter() {
        if r < t {
            return t;
        }
    }
    rate
}

/// Next speed for Slower.
pub fn rate_slower(rate: f32) -> f32 {
    let r = rate * SLOWER_FACTOR;
    for i in 1..RATES.len() {
        if r <= RATES[i] {
            return RATES[i - 1];
        }
    }
    rate
}

/// Fine speed change by one tenth.
pub fn rate_fine(rate: f32, dir: i32) -> f32 {
    let sign = if rate < 0.0 { -1.0 } else { 1.0 };
    let r = ((rate.abs() as f64) / FINE_STEP + dir as f64 + 0.05).floor() * FINE_STEP;
    let r = (r as f32).clamp(FINE_MIN, FINE_MAX);
    r * sign
}

/// Aspect ratios cycled by the A key ("Default" first).
pub const ASPECT_RATIOS: [(&str, f32); 9] = [
    ("Default", 0.0),
    ("16:9", 16.0 / 9.0),
    ("4:3", 4.0 / 3.0),
    ("1:1", 1.0),
    ("16:10", 16.0 / 10.0),
    ("2.21:1", 2.21),
    ("2.35:1", 2.35),
    ("2.39:1", 2.39),
    ("5:4", 5.0 / 4.0),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates() {
        assert_eq!(rate_faster(1.0), 1.5);
        assert_eq!(rate_faster(1.5), 2.0);
        assert_eq!(rate_slower(1.0), 2.0 / 3.0);
        assert_eq!(rate_slower(2.0), 1.5);
        assert!((rate_fine(1.0, 1) - 1.1).abs() < 1e-5);
        assert!((rate_fine(1.0, -1) - 0.9).abs() < 1e-5);
    }

    #[test]
    fn subtitle_scale() {
        assert_eq!(sub_scale_step(100, true), 110);
        assert_eq!(sub_scale_step(100, false), 90);
        assert_eq!(sub_scale_step(150, true), 175);
        assert_eq!(sub_scale_step(50, false), 25);
        assert_eq!(sub_scale_step(500, true), 500);
        assert_eq!(sub_scale_step(25, false), 25);
    }

    #[test]
    fn volume() {
        assert!((volume_after_steps(1.0, 1) - 1.05).abs() < 1e-5);
        assert!((volume_after_steps(1.98, 1) - 2.0).abs() < 1e-5);
        assert_eq!(volume_after_steps(0.02, -1), 0.0);
    }
}
