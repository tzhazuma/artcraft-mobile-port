//! 探针应用（Android）：触屏 UI 演示 + MediaCodec 硬解测试。
//!
//! 目标是把「移动端需要什么」用最小代码验证一遍：
//! - 触屏交互：轻点 / 拖动平移 / 双指缩放 / 双击复位 / 长按菜单（egui 多指手势）
//! - 触控布局：底部大按钮工具条、状态栏、可滚动信息区（触控目标 ≥ 48dp）
//! - 硬解：MediaCodec（AMediaCodec）解码内嵌 H.264，对应上游 `crates/platform` 的 Android 对应物
//!
//! 注意：eframe 0.36 的 `App` trait 是 `logic`（每帧逻辑）+ `ui`（必需，画界面）两段式。

use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver};

use eframe::egui;

#[cfg(target_os = "android")]
mod mediacodec;

const LOG_MAX: usize = 6;

struct TouchApp {
    zoom: f32,
    pan: egui::Vec2,
    log: VecDeque<String>,
    playing: bool,
    tool: usize,
    frames: u64,
    hw_result: Option<String>,
    hw_rx: Option<Receiver<String>>,
    last_touch_count: usize,
}

impl Default for TouchApp {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            log: VecDeque::new(),
            playing: false,
            tool: 0,
            frames: 0,
            hw_result: None,
            hw_rx: None,
            last_touch_count: 0,
        }
    }
}

impl TouchApp {
    fn push_log(&mut self, s: impl Into<String>) {
        self.log.push_front(s.into());
        while self.log.len() > LOG_MAX {
            self.log.pop_back();
        }
    }
}

#[cfg(target_os = "android")]
fn run_hw_test() -> String {
    let report = mediacodec::run_decode_test();
    let summary = report.summary();
    // 同时写 logcat，便于在无界面/被遮挡时取证
    log::info!("probe: MediaCodec 硬解测试结果 — {}", summary.replace('\n', " | "));
    summary
}

#[cfg(not(target_os = "android"))]
fn run_hw_test() -> String {
    "MediaCodec 只在 Android 上可用".to_string()
}

impl eframe::App for TouchApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames += 1;

        // ---------- 顶部状态栏 ----------
        ui.horizontal(|ui| {
            ui.heading("FilmCraft 移动端探针");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{:.0}%", self.zoom * 100.0));
                ui.separator();
                ui.label(format!("帧 {}", self.frames));
            });
        });
        ui.separator();

        // ---------- 画布（手势区） ----------
        let bottom_h = 108.0;
        let avail = ui.available_size();
        let canvas_size = egui::vec2(avail.x, (avail.y - bottom_h).max(140.0));
        let (rect, resp) = ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());

        if resp.dragged() {
            let d = resp.drag_delta();
            if d.length_sq() > 1.0 {
                self.pan += d;
            }
        }
        let zoom_delta = ctx.input(|i| i.zoom_delta());
        if (zoom_delta - 1.0).abs() > 0.001 {
            self.zoom = (self.zoom * zoom_delta).clamp(0.25, 6.0);
            self.push_log(format!("双指缩放 → {:.0}%", self.zoom * 100.0));
        }
        if let Some(mt) = ctx.input(|i| i.multi_touch()) {
            if mt.num_touches != self.last_touch_count {
                self.last_touch_count = mt.num_touches;
                self.push_log(format!("多指接触：{} 指", mt.num_touches));
            }
        } else {
            self.last_touch_count = 0;
        }
        if resp.clicked() {
            self.push_log("轻点（选择）");
        }
        if resp.double_clicked() {
            self.zoom = 1.0;
            self.pan = egui::Vec2::ZERO;
            self.push_log("双击：视图复位");
        }
        if resp.long_touched() {
            self.push_log("长按：上下文菜单（演示）");
        }

        // ---------- 画布绘制 ----------
        let painter = ui.painter_at(rect);
        let step = 48.0 * self.zoom;
        let grid = egui::Stroke::new(1.0, egui::Color32::from_gray(58));
        let mut x = rect.left() + self.pan.x.rem_euclid(step);
        while x < rect.right() {
            painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], grid);
            x += step;
        }
        let mut y = rect.top() + self.pan.y.rem_euclid(step);
        while y < rect.bottom() {
            painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], grid);
            y += step;
        }
        let center = rect.center() + self.pan;
        let clip = egui::Rect::from_center_size(center, egui::vec2(320.0 * self.zoom, 180.0 * self.zoom));
        let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(110, 190, 255));
        for (a, b) in [
            (clip.left_top(), clip.right_top()),
            (clip.right_top(), clip.right_bottom()),
            (clip.right_bottom(), clip.left_bottom()),
            (clip.left_bottom(), clip.left_top()),
        ] {
            painter.line_segment([a, b], stroke);
        }
        painter.line_segment(
            [egui::pos2(center.x - 12.0, center.y), egui::pos2(center.x + 12.0, center.y)],
            egui::Stroke::new(1.5, egui::Color32::from_gray(180)),
        );
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            format!("clip {:.0}%", self.zoom * 100.0),
            egui::FontId::proportional(20.0),
            egui::Color32::WHITE,
        );
        painter.text(
            rect.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            "拖动=平移 · 双指=缩放 · 双击=复位 · 长按=菜单",
            egui::FontId::proportional(13.0),
            egui::Color32::from_gray(150),
        );

        // ---------- 手势日志 ----------
        ui.separator();
        for (i, line) in self.log.iter().enumerate() {
            let alpha = 255 - (i as u8) * 34;
            ui.colored_label(egui::Color32::from_rgba_unmultiplied(200, 220, 255, alpha), line);
        }

        // ---------- 硬解测试 ----------
        let running = self.hw_rx.is_some();
        let label = if running { "硬解测试运行中…" } else { "运行 MediaCodec 硬解测试" };
        if ui
            .add_sized([ui.available_width(), 52.0], egui::Button::new(label))
            .clicked()
            && !running
        {
            let (tx, rx) = channel();
            self.hw_rx = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(run_hw_test());
            });
        }
        if let Some(rx) = &self.hw_rx {
            match rx.try_recv() {
                Ok(msg) => {
                    self.push_log("硬解测试完成");
                    self.hw_result = Some(msg);
                    self.hw_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.hw_rx = None,
            }
        }
        if let Some(res) = &self.hw_result {
            ui.add(egui::Label::new(egui::RichText::new(res).monospace().size(13.0)).wrap());
        }

        // ---------- 底部触控工具条 ----------
        ui.separator();
        ui.horizontal(|ui| {
            let b = |ui: &mut egui::Ui, text: &str, active: bool| -> bool {
                let fill = if active { egui::Color32::from_rgb(70, 110, 180) } else { egui::Color32::from_gray(45) };
                ui.add_sized([92.0, 64.0], egui::Button::new(egui::RichText::new(text).size(17.0)).fill(fill)).clicked()
            };
            if b(ui, "选择", self.tool == 0) { self.tool = 0; self.push_log("工具：选择"); }
            if b(ui, "切片", self.tool == 1) { self.tool = 1; self.push_log("工具：切片"); }
            if b(ui, "缩放", self.tool == 2) { self.tool = 2; self.push_log("工具：缩放"); }
            let play = if self.playing { "暂停" } else { "播放" };
            if b(ui, play, self.playing) {
                self.playing = !self.playing;
                self.push_log(if self.playing { "播放" } else { "暂停" });
            }
            if b(ui, "导出", false) { self.push_log("导出（演示按钮）"); }
        });

        if self.playing {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
    }
}

/// 把设备自带的中文字体挂到 egui 的字体回退链上（默认字体没有 CJK，会渲染成方块）。
/// 上游的做法是随包带 craft-fonts；这里演示「运行时读系统字体」的轻量方案。
#[cfg(target_os = "android")]
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
                fonts
                    .font_data
                    .insert("system-cjk".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
                // 放在默认字体之后：拉丁字形仍用 egui 自带字体，CJK 走系统字体
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts.families.entry(family).or_default().push("system-cjk".to_owned());
                }
                ctx.set_fonts(fonts);
                log::info!("probe: loaded system CJK font {path}");
                return;
            }
            Err(e) => log::warn!("probe: font {path} unavailable: {e}"),
        }
    }
    log::warn!("probe: no system CJK font found; CJK text will be tofu");
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("egui-android-probe: android_main entered");
    // 模拟器（SwiftShader/ANGLE）上 wgpu 的 Vulkan 路径会崩在模拟器驱动里，先强制 GLES 后端。
    // 真机上可以去掉这行，让 wgpu 选 Vulkan。
    std::env::set_var("WGPU_BACKEND", "gl");
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    match eframe::run_native(
        "EguiProbe",
        options,
        Box::new(|cc| {
            install_system_font(&cc.egui_ctx);
            Ok(Box::new(TouchApp::default()))
        }),
    ) {
        Ok(()) => log::info!("eframe exited normally"),
        Err(e) => log::error!("eframe failed: {e}"),
    }
}
