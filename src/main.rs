#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod compare;
mod diff;
mod fsops;
mod highlight;
mod jobs;
mod model;
mod paths;
mod remote;
mod session;
mod sshcfg;
mod textutil;
mod theme;
mod transfer;
mod url;

use std::sync::Arc;

fn main() -> eframe::Result {
    let runtime = Arc::new(jobs::Runtime::new());
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([1020.0, 680.0])
            .with_title("CompareShow"),
        ..Default::default()
    };
    eframe::run_native(
        "CompareShow",
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(app::CompareApp::new(runtime)))
        }),
    )
}
