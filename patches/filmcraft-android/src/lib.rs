//! FilmCraft on Android — GameActivity + eframe/wgpu, reusing the desktop engine and egui UI.
//!
//! Nothing here is Android-specific beyond the entry point, the data directory, the touch shell
//! and the font fallback: the same `filmcraft_engine::Session` and `filmcraft_ui_egui::FilmcraftApp`
//! the desktop shell builds are wired up here. What the desktop shell gets from the OS, this crate
//! takes from the activity:
//!
//! | Desktop (`apps/filmcraft`) | Android (this crate) |
//! |---|---|
//! | `std::env::args()` launch flags | [`winit::platform::android::activity::AndroidApp`] |
//! | data dir from `default_data_dir()` | [`AndroidApp::internal_data_path`] |
//! | rfd file dialogs | `HostHooks::default()` (no pickers yet; SAF bridge is follow-up work) |
//! | cpal audio output | not wired yet (cpal's Android backend exists; needs permissions) |
//! | VideoToolbox hardware decode | not wired yet (`crates/platform` needs a MediaCodec backend) |
//! | desktop-sized UI | [`TouchShell`] scales the UI up and enlarges touch targets |
//!
//! The engine, the UI and the theme are untouched: the shell only adjusts egui's scale and spacing
//! *after* FilmCraft installs its own theme on the first frame, so the desktop layout stays the
//! single source of truth.
#![cfg(target_os = "android")]

use filmcraft_engine::Session;
use filmcraft_engine::autosave::AutosaveConfig;
use filmcraft_ui_egui::FilmcraftApp;
use winit::platform::android::activity::AndroidApp;

/// How much to scale the desktop UI up on a phone before the layout reflows.
///
/// Measured on a 1080×2400 @ 420 dpi phone: egui's logical size is 411×914 pt at
/// `pixels_per_point = 2.625`, while the desktop layout is designed for ≥900×560 pt. Scaling up
/// (tried 1.35) only shrinks the logical viewport further and makes the panels overlap more, so
/// the shell does **not** scale: the layout needs a rework (bottom toolbars, one panel at a time),
/// not a zoom. Keep this knob at 1.0 until that rework exists; see `EXPERIMENT.md` §5.5.
const TOUCH_SCALE: f32 = 1.0;

/// Touch shell: forward everything to [`FilmcraftApp`], then apply the phone adjustments once,
/// after FilmCraft's own theme is in place.
struct TouchShell {
    inner: FilmcraftApp,
    tuned: bool,
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
    }
}

/// Put the device's CJK font at the end of egui's fallback chain: the bundled fonts have no CJK
/// glyphs, so Chinese/Japanese text would render as tofu. (Upstream ships craft-fonts instead;
/// reading `/system/fonts` needs no extra download.)
fn install_system_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/system/fonts/NotoSansCJK-Regular.ttc",
        "/system/fonts/NotoSerifCJK-Regular.ttc",
        "/system/fonts/DroidSansFallback.ttf",
    ];
    for path in CANDIDATES {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut fonts = egui::FontDefinitions::default();
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

mod selftest;

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
    // Decode the embedded clip through the registered factories: logcat evidence that the Android
    // backend works end to end (decoder name, picture count, hardware counters).
    std::thread::spawn(|| log::info!("filmcraft-android: {}", selftest::run()));
    // Application-private storage; used for auto-save, crash logs and preferences.
    let data_dir = app.internal_data_path();
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let started = eframe::run_native(
        "FilmCraft",
        options,
        Box::new(move |cc| {
            install_system_font(&cc.egui_ctx);
            let mut session = Session::default();
            if let Some(dir) = data_dir {
                if let Err(e) = session.start_autosave(AutosaveConfig::new(dir)) {
                    log::warn!("filmcraft-android: auto-save unavailable: {e}");
                }
            }
            let mut app = FilmcraftApp::new(session);
            if let Some(rs) = cc.wgpu_render_state.clone() {
                app.set_wgpu(rs);
            }
            // File dialogs, "open path" and the rest of `HostHooks` stay unset for now: import
            // and export need a Storage Access Framework bridge, tracked as follow-up work.
            Ok(Box::new(TouchShell { inner: app, tuned: false }))
        }),
    );
    if let Err(e) = started {
        log::error!("filmcraft-android: eframe exited: {e}");
    }
}
