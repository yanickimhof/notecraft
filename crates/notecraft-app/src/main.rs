//! NoteCraft desktop app.
//!
//! Usage: `notecraft-app [--workspace <folder>] [--dark] [--screenshot <file.png>] [--export-icns <file.icns>]`
//!
//! `--screenshot` opens a sample workspace without taking focus, draws about
//! 120 frames, saves the window as a PNG and exits (used for automated checks).

mod ai_panel;
mod app;
mod backups;
mod dbview;
mod disk;
mod editor;
mod finder;
mod graph_view;
mod i18n;
mod icons;
mod math;
mod mcp_setup;
mod meeting;
mod mermaid;
mod mermaid_more;
mod pageview;
mod publish;
mod present;
mod svg_export;
mod remind;
mod settings;
mod sidebar;
mod theme;
mod tray;
mod vault_ui;
mod widgets;

use app::{Args, NcApp};
use eframe::egui;
use std::path::PathBuf;

const ICON_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.png"));
const ICON_ICNS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/NoteCraft.icns"));

fn parse_args() -> Result<(Args, Option<PathBuf>), String> {
    let mut args = Args::default();
    let mut icns = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => args.screenshot = Some(it.next().ok_or("--screenshot needs a file")?.into()),
            "--workspace" => args.workspace = Some(it.next().ok_or("--workspace needs a folder")?.into()),
            "--export-icns" => icns = Some(PathBuf::from(it.next().ok_or("--export-icns needs a file")?)),
            "--dark" => args.dark = true,
            "--scroll" => args.scroll = it.next().and_then(|v| v.parse().ok()),
            "--open" => args.open = Some(it.next().ok_or("--open needs a page title")?),
            "--view" => args.view = Some(it.next().ok_or("--view needs table, board or list")?),
            "--panel" => args.panel = Some(it.next().ok_or("--panel needs ai or settings")?),
            "--version" => {
                println!("NøteCraft {}", notecraft_core::version());
                std::process::exit(0);
            }
            "-h" | "--help" => {
                println!("notecraft-app [--workspace <folder>] [--dark] [--screenshot <file.png>] [--export-icns <file.icns>]");
                std::process::exit(0);
            }
            other if !other.starts_with('-') && args.workspace.is_none() => args.workspace = Some(other.into()),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok((args, icns))
}

fn main() -> eframe::Result {
    notecraft_core::log::install_panic_hook();
    let (args, icns) = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if let Some(path) = icns {
        if let Err(e) = std::fs::write(&path, ICON_ICNS) {
            eprintln!("could not write {}: {e}", path.display());
            std::process::exit(1);
        }
        return Ok(());
    }
    set_process_name();
    let screenshot = args.screenshot.is_some();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("NøteCraft")
        .with_app_id("notecraft")
        .with_inner_size(theme::DEFAULT_WINDOW_SIZE)
        .with_min_inner_size(theme::MIN_WINDOW_SIZE)
        .with_fullsize_content_view(true)
        .with_title_shown(false)
        .with_titlebar_shown(false);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON_PNG) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    if screenshot {
        // never steal focus from what the person is doing
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let mut options = eframe::NativeOptions { viewport, persist_window: !screenshot, ..Default::default() };
    if screenshot {
        options.event_loop_builder = Some(Box::new(|builder| {
            #[cfg(target_os = "macos")]
            {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                builder.with_activate_ignoring_other_apps(false);
            }
            let _ = builder;
        }));
    }
    eframe::run_native("NoteCraft", options, Box::new(move |cc| Ok(Box::new(NcApp::new(cc, args)))))
}

/// The application menu (About, Hide, Quit) takes its name from the process, which is the ASCII
/// executable name; the brand spelling is set here so the menu matches the title in the menu bar.
#[cfg(target_os = "macos")]
fn set_process_name() {
    use objc2_foundation::{NSProcessInfo, NSString};
    // SAFETY: plain Foundation calls on the main thread, before the event loop starts.
    unsafe { NSProcessInfo::processInfo().setProcessName(&NSString::from_str("NøteCraft")) };
}

#[cfg(not(target_os = "macos"))]
fn set_process_name() {}
