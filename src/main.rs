#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cache;
mod compare;
mod fold;
mod fonts;
mod highlight;
mod icons;
mod i18n;
mod image_preview;
mod keymap;
mod model;
mod tab;
mod theme;
mod tree;
mod updater;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result<()> {
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let msg = format!("PANIC: {info}\nBacktrace:\n{backtrace:?}");
        eprintln!("{msg}");
        let _ = std::fs::write("/tmp/ijson_panic.log", &msg);
    }));

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1150.0, 750.0])
        .with_min_inner_size([800.0, 420.0])
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
            #[cfg(target_os = "macos")]
            disable_native_titlebar_double_click(cc);
            theme::setup_theme(&cc.egui_ctx);
            Ok(Box::new(app::JsonViewerApp::new(&cc.egui_ctx)))
        }),
    )
}

#[cfg(target_os = "macos")]
fn disable_native_titlebar_double_click(cc: &eframe::CreationContext<'_>) {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool, Sel};
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

    let Ok(handle) = cc.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let ns_view = appkit.ns_view.as_ptr() as *mut AnyObject;
    if ns_view.is_null() {
        return;
    }

    unsafe {
        // With FullSizeContentView, NSThemeFrame still attaches a titlebar double-click
        // gesture recognizer across the top 32pt, so double-clicking toolbar buttons zooms
        // the window. Disable it so only our window_drag_area (empty space) toggles maximize.
        let theme_frame: *mut AnyObject = msg_send![ns_view, superview];
        if theme_frame.is_null() {
            return;
        }
        let gestures: *mut AnyObject = msg_send![theme_frame, gestureRecognizers];
        if gestures.is_null() {
            return;
        }
        let count: usize = msg_send![gestures, count];
        for i in 0..count {
            let gesture: *mut AnyObject = msg_send![gestures, objectAtIndex: i];
            if gesture.is_null() {
                continue;
            }
            let action: Option<Sel> = msg_send![gesture, action];
            if let Some(sel) = action {
                if sel.name() == "_handleTitledFrameDoubleClickGesture:" {
                    let _: () = msg_send![gesture, setEnabled: Bool::NO];
                }
            }
        }
    }
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
