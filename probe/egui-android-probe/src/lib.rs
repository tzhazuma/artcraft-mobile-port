//! 最小 eframe + wgpu Android 宿主（GameActivity）
//! 目的：验证 FilmCraft 同款技术链路（eframe 0.36 + wgpu + winit 0.30 + GameActivity）在 Android 上可运行。

use eframe::egui;

struct ProbeApp {
    frames: u64,
}

impl eframe::App for ProbeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("egui + wgpu on Android");
            ui.separator();
            ui.label(format!("frame: {}", self.frames));
            ui.label("FilmCraft 同款链路：eframe 0.36 + wgpu + winit 0.30 + GameActivity");
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("egui-android-probe: android_main entered");
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    match eframe::run_native(
        "EguiProbe",
        options,
        Box::new(|_cc| Ok(Box::new(ProbeApp { frames: 0 }))),
    ) {
        Ok(()) => log::info!("eframe exited normally"),
        Err(e) => log::error!("eframe failed: {e}"),
    }
}
