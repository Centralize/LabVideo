#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ac3;
mod app;
mod audio_out;
mod icons;
mod mechanics;
mod media;
mod player;
mod settings;
mod subtitles;

use std::path::PathBuf;

pub const APP_NAME: &str = "Lab Video Player";

fn main() -> eframe::Result<()> {
    let mut screenshot: Option<PathBuf> = None;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut test = app::TestOptions::default();
    let mut args = std::env::args().skip(1).peekable();
    while let Some(a) = args.next() {
        if a == "--screenshot" {
            screenshot = args.next().map(PathBuf::from);
        } else if a == "--start" {
            test.start = args.next().and_then(|s| s.parse().ok());
        } else if a == "--frames" {
            test.frames = args.next().and_then(|s| s.parse().ok());
        } else if a == "--keys" {
            test.keys = args.next().unwrap_or_default();
        } else if let Some(p) = a.strip_prefix("--screenshot=") {
            screenshot = Some(PathBuf::from(p));
        } else if a.starts_with("--language") {
            // WinMain (vlc.exe 0x1400017ff): "--language" takes the next argument as its value
            // unless that one is another option; "--language=xx" carries it inline.
            if a == "--language" && args.peek().map(|n| !n.starts_with("--")).unwrap_or(false) {
                args.next();
            }
        } else if a == "--no-crashdump" {
            // only switches off the original's crash reporter, which this player doesn't have
        } else if !a.starts_with("--") {
            // everything else on the command line is media to play, started right away
            // (libvlc_playlist_play after start-up, vlc.exe 0x140001939)
            files.push(PathBuf::from(a));
        }
    }

    let icon = app::window_icon();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_inner_size([860.0, 560.0])
        .with_min_inner_size([420.0, 260.0])
        .with_drag_and_drop(true)
        .with_icon(icon);
    if screenshot.is_some() {
        // a check: don't take focus, stay behind other windows
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    // like the original on a first run: its natural size, centred on the screen
    let mut options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    #[cfg(target_os = "macos")]
    if screenshot.is_some() {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    options.run_and_return = true;

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::PlayerApp::new(cc, files, screenshot, test)))),
    )
}
