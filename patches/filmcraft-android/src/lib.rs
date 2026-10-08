//! FilmCraft on Android — GameActivity + eframe/wgpu, reusing the desktop engine and egui UI.
//!
//! Nothing here is Android-specific beyond the entry point, the data directory, the touch shell,
//! the action bar and the font fallback: the same `filmcraft_engine::Session` and
//! `filmcraft_ui_egui::FilmcraftApp` the desktop shell builds are wired up here. What the desktop
//! shell gets from the OS, this crate takes from the activity:
//!
//! | Desktop (`apps/filmcraft`) | Android (this crate) |
//! |---|---|
//! | `std::env::args()` launch flags | [`winit::platform::android::activity::AndroidApp`] |
//! | data dir from `default_data_dir()` | [`AndroidApp::internal_data_path`] |
//! | rfd file dialogs | [`TouchShell`]'s action bar: the SAF picker (Java) copies into the app dir and the paths go to the engine over the control channel |
//! | cpal audio output | not wired yet (cpal's Android backend exists; needs permissions) |
//! | VideoToolbox hardware decode | MediaCodec in `crates/platform` (see `patches/platform-mediacodec`) |
//! | desktop-sized UI | [`TouchShell`] enlarges touch targets and adds the action bar |
//!
//! The engine, the UI and the theme are untouched: the shell only adjusts egui's scale and spacing
//! *after* FilmCraft installs its own theme on the first frame, so the desktop layout stays the
//! single source of truth.
#![cfg(target_os = "android")]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use filmcraft_engine::Session;
use filmcraft_engine::autosave::AutosaveConfig;
use filmcraft_ui_egui::{ControlRequest, FilmcraftApp};
use winit::platform::android::activity::AndroidApp;

mod saf;
mod selftest;

/// How much to scale the desktop UI up on a phone before the layout reflows.
///
/// Measured on a 1080×2400 @ 420 dpi phone: egui's logical size is 411×914 pt at
/// `pixels_per_point = 2.625`, while the desktop layout is designed for ≥900×560 pt. Scaling up
/// (tried 1.35) only shrinks the logical viewport further and makes the panels overlap more, so
/// the shell does **not** scale: the layout needs a rework (bottom toolbars, one panel at a time),
/// not a zoom. Keep this knob at 1.0 until that rework exists; see `EXPERIMENT.md` §5.5.
const TOUCH_SCALE: f32 = 1.0;

/// Touch shell: forward everything to [`FilmcraftApp`], then apply the phone adjustments once
/// (after FilmCraft's own theme is in place) and draw the phone's action bar.
struct TouchShell {
    inner: FilmcraftApp,
    tuned: bool,
    /// The CJK fallback font has been added (after the theme's fonts went live).
    font_done: bool,
    /// Commands for the app's control channel — the same entry point the desktop control server
    /// and MCP use — so the action bar can drive the engine without blocking on a file dialog.
    control: Sender<ControlRequest>,
    /// A picker is open (or the picked files are being imported).
    busy: Arc<AtomicBool>,
    /// The app's internal data dir (`filesDir`): the Java SAF bridge writes its manifest there.
    data_dir: Option<PathBuf>,
    /// The running activity (`AndroidApp::activity_as_ptr()`), for the SAF picker.
    activity: usize,
}

impl TouchShell {
    fn tune(ctx: &egui::Context) {
        let before = ctx.pixels_per_point();
        if (TOUCH_SCALE - 1.0).abs() > f32::EPSILON {
            ctx.set_pixels_per_point((before * TOUCH_SCALE).min(4.0));
        }
        ctx.all_styles_mut(|style| {
            // Touch targets: comfortable minimum, roomier padding, a little more separation so
            // neighbouring rows are harder to hit by accident.
            style.spacing.interact_size = egui::vec2(40.0, 40.0);
            style.spacing.button_padding = egui::vec2(10.0, 8.0);
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.slider_width = style.spacing.slider_width.max(160.0);
            // Tooltips should not pop up from a resting finger.
            style.interaction.show_tooltips_only_when_still = true;
        });
        let rect = ctx.viewport_rect();
        log::info!(
            "filmcraft-android: touch mode applied (ppp {before}, logical {}×{} pt, interact {} pt)",
            rect.width().round(),
            rect.height().round(),
            ctx.style_of(ctx.theme()).spacing.interact_size.x,
        );
    }

    /// The phone's floating action bar: the entry points that a phone needs and the desktop menus
    /// cannot offer — import media and open a project through the system picker, then hand the
    /// copied paths to the engine over the control channel.
    fn action_bar(&mut self, ctx: &egui::Context) {
        let busy = self.busy.load(Ordering::Relaxed);
        egui::Area::new(egui::Id::new("touch-action-bar"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -108.0))
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    let import = if busy { "导入中…" } else { "导入媒体" };
                    if ui
                        .add_enabled(!busy, egui::Button::new(egui::RichText::new(import).size(20.0)).min_size(egui::vec2(150.0, 56.0)))
                        .clicked()
                    {
                        self.pick("file.import", "paths");
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(egui::RichText::new("打开工程").size(20.0)).min_size(egui::vec2(150.0, 56.0)))
                        .clicked()
                    {
                        self.pick("file.open", "path");
                    }
                });
            });
    }

    /// Run the SAF picker on a background thread and hand the result to the engine's control
    /// channel (the request is executed on the UI thread by `FilmcraftApp` itself).
    fn pick(&self, method: &'static str, key: &'static str) {
        if self.busy.swap(true, Ordering::Relaxed) {
            return;
        }
        let Some(data_dir) = self.data_dir.clone() else {
            log::warn!("SAF: no data dir; cannot import");
            self.busy.store(false, Ordering::Relaxed);
            return;
        };
        let control = self.control.clone();
        let busy = self.busy.clone();
        let activity = self.activity;
        std::thread::spawn(move || {
            let paths = saf::pick_media(activity, &data_dir, std::time::Duration::from_secs(300));
            if paths.is_empty() {
                log::info!("SAF: nothing picked");
            } else {
                let params = if key == "path" { serde_json::json!({ key: paths[0] }) } else { serde_json::json!({ key: paths }) };
                let (request, _reply) = ControlRequest::new(method, params);
                match control.send(request) {
                    Ok(()) => log::info!("SAF: sent {method} for {} file(s)", paths.len()),
                    Err(_) => log::warn!("SAF: control channel is closed"),
                }
            }
            busy.store(false, Ordering::Relaxed);
        });
    }
}

impl eframe::App for TouchShell {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.inner.ui(ui, frame);
        if !self.tuned {
            // The first `ui` pass runs FilmCraft's theme install, so adjust after it.
            self.tuned = true;
            Self::tune(ui.ctx());
        }
        if !self.font_done && Self::theme_fonts_live(ui.ctx()) {
            // The theme's fonts only become visible once egui has rebuilt them, a frame after the
            // theme install: adding the CJK fallback any earlier would drop the theme's families
            // (egui panics on a family that is bound to no font).
            self.font_done = true;
            install_system_font(ui.ctx());
        }
        self.action_bar(ui.ctx());
    }
}

impl TouchShell {
    /// Whether the theme's own fonts are live (it defines named families such as "semibold").
    fn theme_fonts_live(ctx: &egui::Context) -> bool {
        ctx.fonts(|f| f.definitions().families.keys().any(|k| matches!(k, egui::FontFamily::Name(_))))
    }
}

/// Add the device's CJK font to egui's *current* fallback chain: the bundled fonts have no CJK
/// glyphs, so Chinese/Japanese text would render as tofu. This must run **after** FilmCraft's theme
/// install (which replaces the fonts), and it edits the current definitions rather than starting
/// from the defaults, so the theme's own fonts stay.
fn install_system_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/system/fonts/NotoSansCJK-Regular.ttc",
        "/system/fonts/NotoSerifCJK-Regular.ttc",
        "/system/fonts/DroidSansFallback.ttf",
    ];
    for path in CANDIDATES {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut fonts = ctx.fonts(|f| f.definitions().clone());
                fonts.font_data.insert(
                    "system-cjk".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts.families.entry(family).or_default().push("system-cjk".to_owned());
                }
                ctx.set_fonts(fonts);
                log::info!("filmcraft-android: loaded system CJK font {path}");
                return;
            }
            Err(e) => log::warn!("filmcraft-android: font {path} unavailable: {e}"),
        }
    }
    log::warn!("filmcraft-android: no system CJK font found; CJK text will be tofu");
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("filmcraft-android: android_main entered");
    // Hardware video decoding (MediaCodec). Streams it does not take fall through to FilmCraft's
    // own decoders, so registering never makes a file undecodable.
    let hardware = filmcraft_platform::register();
    log::info!("filmcraft-android: hardware decoding: {hardware:?}");
    // Decode the embedded clips through the registered factories: logcat evidence that the Android
    // backend works end to end (decoder name, picture count, hardware counters).
    std::thread::spawn(|| log::info!("filmcraft-android: {}", selftest::run()));
    // Application-private storage; used for auto-save, crash logs and preferences.
    let data_dir = app.internal_data_path();
    // The Java activity object: `ndk_context` only hands out the Application, which cannot start
    // the document picker for a result.
    let activity = app.activity_as_ptr() as usize;
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    // The control channel: the action bar sends `file.import` / `file.open` from background threads
    // and `FilmcraftApp` executes them on the UI thread, exactly like the desktop control server.
    let (control, requests) = std::sync::mpsc::channel::<ControlRequest>();
    let started = eframe::run_native(
        "FilmCraft",
        options,
        Box::new(move |cc| {
            let mut session = Session::default();
            if let Some(dir) = data_dir.clone() {
                if let Err(e) = session.start_autosave(AutosaveConfig::new(dir)) {
                    log::warn!("filmcraft-android: auto-save unavailable: {e}");
                }
            }
            let mut app = FilmcraftApp::new(session).with_control(requests);
            if let Some(rs) = cc.wgpu_render_state.clone() {
                app.set_wgpu(rs);
            }
            // FilmCraft's own File ▸ Import items still have no picker (they go through the
            // synchronous `HostHooks`); the action bar is the Android way in until that is wired.
            Ok(Box::new(TouchShell { inner: app, tuned: false, font_done: false, control, busy: Arc::new(AtomicBool::new(false)), data_dir, activity }))
        }),
    );
    if let Err(e) = started {
        log::error!("filmcraft-android: eframe exited: {e}");
    }
}
