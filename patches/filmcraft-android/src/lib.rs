//! FilmCraft on Android — GameActivity + eframe/wgpu, reusing the desktop engine and egui UI.
//!
//! Nothing here is Android-specific beyond the entry point and the data directory: the same
//! `filmcraft_engine::Session` and `filmcraft_ui_egui::FilmcraftApp` the desktop shell builds are
//! wired up here. What the desktop shell gets from the OS, this crate takes from the activity:
//!
//! | Desktop (`apps/filmcraft`) | Android (this crate) |
//! |---|---|
//! | `std::env::args()` launch flags | [`winit::platform::android::activity::AndroidApp`] |
//! | data dir from `default_data_dir()` | [`AndroidApp::internal_data_path`] |
//! | rfd file dialogs | `HostHooks::default()` (no pickers yet; SAF bridge is follow-up work) |
//! | cpal audio output | not wired yet (cpal's Android backend exists; needs permissions) |
//! | VideoToolbox hardware decode | unavailable (pure-Rust decoders only) |
#![cfg(target_os = "android")]

use filmcraft_engine::Session;
use filmcraft_engine::autosave::AutosaveConfig;
use filmcraft_ui_egui::FilmcraftApp;
use winit::platform::android::activity::AndroidApp;

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("filmcraft-android: android_main entered");
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
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = started {
        log::error!("filmcraft-android: eframe exited: {e}");
    }
}
