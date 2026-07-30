use anyhow::Result;
use std::borrow::Cow;
pub mod device;
pub mod log_parse;
pub mod root_view;
pub mod tabs;
pub mod theme;
pub mod toast;

use gpui::{App, AppContext, AssetSource, Bounds, SharedString, WindowOptions, px, size};
use gpui_component::theme::{Theme, ThemeMode};
use rust_embed::RustEmbed;

#[derive(Clone, Copy, RustEmbed)]
#[folder = "assets"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(<Assets as RustEmbed>::get(path).map(|f| f.data))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(<Assets as RustEmbed>::iter()
            .filter(|p| p.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

fn main() {
    env_logger::init();

    gpui::Application::new()
        .with_assets(Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);

            let bounds = Bounds::centered(None, size(px(780.), px(540.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Logger Client".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    Theme::change(ThemeMode::Dark, Some(window), cx);
                    theme::apply(cx);
                    let view = cx.new(|cx| root_view::RootView::new(window, cx));
                    cx.new(|cx| gpui_component::Root::new(view, window, cx))
                },
            )
            .expect("failed to open window");

            cx.activate(true);
        });
}
