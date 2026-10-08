//! FilmCraft on Android — GameActivity + eframe/wgpu, reusing the desktop engine and egui UI.
//!
//! Nothing here is Android-specific beyond the entry point, the data directory, the touch size
//! adjustments and the font fallback: the same `filmcraft_engine::Session` and
//! `filmcraft_ui_egui::FilmcraftApp` the desktop shell builds are wired up here. What the desktop
//! shell gets from the OS, this crate takes from the activity:
//!
//! | Desktop (`apps/filmcraft`) | Android (this crate) |
//! |---|---|
//! | `std::env::args()` launch flags | [`winit::platform::android::activity::AndroidApp`] |
//! | data dir from `default_data_dir()` | [`AndroidApp::internal_data_path`] |
//! | rfd file dialogs | FilmCraft's own `HostHooks` (File ▸ Import / Open Project) start the SAF picker; the chosen files are copied into the app dir and handed to the engine over the control channel |
//! | cpal audio output | not wired yet (cpal's Android backend exists; needs permissions) |
//! | VideoToolbox hardware decode | MediaCodec in `crates/platform` (see `patches/platform-mediacodec`) |
//! | desktop-sized UI | [`TouchShell`] enlarges touch targets |
//!
//! The engine, the UI and the theme are untouched — the menus, the Import command and the project
//! panel are the desktop ones, so the phone uses the same code paths. The shell only adjusts egui's
//! spacing *after* FilmCraft installs its own theme on the first frame.
#![cfg(target_os = "android")]

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use filmcraft_engine::Session;
use filmcraft_engine::autosave::AutosaveConfig;
use filmcraft_ui_egui::dock::WorkspacePrefs;
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

/// Touch shell: forward everything to [`FilmcraftApp`] and apply the phone adjustments once, after
/// FilmCraft's own theme is in place.
struct TouchShell {
    inner: FilmcraftApp,
    tuned: bool,
    /// The CJK fallback font has been added (after the theme's fonts went live).
    font_done: bool,
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

    /// Whether the theme's own fonts are live (it defines named families such as "semibold").
    fn theme_fonts_live(ctx: &egui::Context) -> bool {
        ctx.fonts(|f| f.definitions().families.keys().any(|k| matches!(k, egui::FontFamily::Name(_))))
    }
}

impl eframe::App for TouchShell {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.logic(ctx, frame);
    }

    /// Diagnostic: whether winit/eframe delivers input at all.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if !raw_input.events.is_empty() {
            log::info!("input: {} event(s), first {:?}", raw_input.events.len(), raw_input.events.first());
        }
        eframe::App::raw_input_hook(&mut self.inner, ctx, raw_input);
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

/// A phone-sized workspace: the program monitor on top and the timeline below, with the project
/// bin and tools as tabs of the same dock. The desktop presets are built for ≥900×560 pt while a
/// phone gives ~411×914 pt (see `EXPERIMENT.md` §5.5), so the shell installs this layout with the
/// app's own workspace API instead of rearranging any panel code.
fn phone_workspace() -> WorkspacePrefs {
    use filmcraft_ui_egui::dock::{DockNode, PanelKind, SavedWorkspace, SplitSize};

    let monitor = DockNode::Tabs { panels: vec![PanelKind::Program], active: 0 };
    let lower = DockNode::Tabs { panels: vec![PanelKind::Timeline, PanelKind::Project, PanelKind::Tools], active: 0 };
    let layout = DockNode::Split { vertical: true, size: SplitSize::Ratio(0.42), a: Box::new(monitor), b: Box::new(lower) };
    WorkspacePrefs {
        saved: vec![SavedWorkspace { name: "Phone".to_owned(), layout: serde_json::to_value(&layout).unwrap_or_default() }],
        current: "Phone".to_owned(),
    }
}

/// Start the SAF picker for one of FilmCraft's own file dialogs.
///
/// `HostHooks` are synchronous (they answer "which files?" inside the frame), and a picker cannot
/// be answered synchronously without freezing the UI thread, so the hook starts this background
/// task and answers "nothing" — the chosen files reach the engine a moment later over the control
/// channel, which runs the *normal* command (`file.import` / `file.open`) on the UI thread.
fn spawn_pick(control: Sender<ControlRequest>, activity: usize, data_dir: Option<PathBuf>, method: &'static str, key: &'static str) {
    let Some(data_dir) = data_dir else {
        log::warn!("SAF: no data dir; cannot open the picker");
        return;
    };
    std::thread::spawn(move || {
        let paths = saf::pick_media(activity, &data_dir, std::time::Duration::from_secs(300));
        if paths.is_empty() {
            log::info!("SAF: nothing picked");
            return;
        }
        // `engine.execute` runs the *engine* command of the same name: `menus::invoke` only opens
        // the file dialog when no paths are given, so this is exactly what File ▸ Import does once
        // the files are known — the desktop import path, with the paths coming from SAF.
        let params = if key == "path" {
            serde_json::json!({"command": method, "params": { key: paths[0] }})
        } else {
            serde_json::json!({"command": method, "params": { key: paths }})
        };
        let (request, reply) = ControlRequest::new("engine.execute", params);
        match control.send(request) {
            Ok(()) => log::info!("SAF: sent {method} for {} file(s)", paths.len()),
            Err(_) => {
                log::warn!("SAF: control channel is closed");
                return;
            }
        }
        match reply.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(value) => log::info!("SAF: {method} replied {value}"),
            Err(e) => log::warn!("SAF: no reply for {method}: {e}"),
        }
    });
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
    // The control channel: SAF results are turned into `file.import` / `file.open` requests from a
    // background thread and `FilmcraftApp` executes them on the UI thread, exactly like the desktop
    // control server.
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
            // A layout that fits a phone; the desktop presets do not (see `phone_workspace`).
            if let Err(e) = app.set_workspaces(phone_workspace()) {
                log::warn!("filmcraft-android: could not install the phone workspace: {e}");
            }
            // The app's own file dialogs (File ▸ Import, Import ▸ Media File, Open Project, …) go
            // through these hooks: the picker opens asynchronously and the picked files come back
            // as regular commands, so the desktop import path is what actually runs.
            {
                let (control, data_dir, activity) = (control.clone(), data_dir.clone(), activity);
                app.hooks.pick_files = Some(Box::new(move |_extensions: &[&str]| {
                    spawn_pick(control.clone(), activity, data_dir.clone(), "file.import", "paths");
                    Vec::new()
                }));
            }
            {
                let (control, data_dir, activity) = (control.clone(), data_dir.clone(), activity);
                app.hooks.pick_open_project = Some(Box::new(move || {
                    spawn_pick(control.clone(), activity, data_dir.clone(), "file.open", "path");
                    None
                }));
            }
            {
                let (control, data_dir, activity) = (control.clone(), data_dir.clone(), activity);
                app.hooks.pick_open_file = Some(Box::new(move |_filter: &str, _extensions: &[&str]| {
                    spawn_pick(control.clone(), activity, data_dir.clone(), "file.import", "paths");
                    None
                }));
            }
            Ok(Box::new(TouchShell { inner: app, tuned: false, font_done: false }))
        }),
    );
    if let Err(e) = started {
        log::error!("filmcraft-android: eframe exited: {e}");
    }
}
