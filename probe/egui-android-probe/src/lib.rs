//! 最小 eframe + wgpu Android 宿主（GameActivity）
//! 目的：验证 FilmCraft 同款技术链路（eframe 0.36 + wgpu + winit 0.30 + GameActivity）在 Android 上可运行。
//!
//! 注意：eframe 0.36 的 `App` trait 是 `logic`（每帧逻辑）+ `ui`（必需，画界面）两段式，
//! 不是旧版的 `update(&Context)`。

use eframe::egui;

struct ProbeApp {
    frames: u64,
}

impl eframe::App for ProbeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frames += 1;
        ui.heading("egui + wgpu on Android");
        ui.separator();
        ui.label(format!("frame: {}", self.frames));
        ui.label("FilmCraft 同款链路：eframe 0.36 + wgpu + winit 0.30 + GameActivity");
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("egui-android-probe: android_main entered");
    // 模拟器（SwiftShader/ANGLE）上 wgpu 的 Vulkan 路径在适配器枚举后 SIGSEGV，
    // 先强制 GLES 后端（真机可去掉这行或按需选择）。
    // SAFETY: 单线程启动阶段设置环境变量，Android 上可用；edition 2021 下非 unsafe。
    std::env::set_var("WGPU_BACKEND", "gl");
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
