mod app;
mod cache;
mod compare;
mod fold;
mod fonts;
mod highlight;
mod icons;
mod i18n;
mod image_preview;
mod model;
mod tab;
mod theme;
mod tree;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1150.0, 750.0])
        .with_min_inner_size([650.0, 420.0])
        .with_title("IJson")
        .with_icon(app_icon());
    #[cfg(target_os = "macos")]
    {
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false);
    }

    if let Some(geom) = cache::read_window_geom() {
        viewport = geom.apply(viewport);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "IJson",
        options,
        Box::new(|cc| {
            theme::setup_theme(&cc.egui_ctx);
            Ok(Box::new(app::JsonViewerApp::new(&cc.egui_ctx)))
        }),
    )
}

fn app_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/IJson.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes.as_slice()));
    let mut reader = decoder.read_info().expect("IJson icon");
    let mut rgba = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut rgba).expect("IJson icon frame");
    rgba.truncate(info.buffer_size());
    egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    }
}
