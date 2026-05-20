use anyhow::{anyhow, Result};
use std::borrow::Cow;
pub mod device;
pub mod root_view;

extern crate warpui;
use rust_embed::RustEmbed;
use warpui::{platform, AssetProvider};

#[derive(Clone, Copy, RustEmbed)]
#[folder = "assets"]
pub struct Assets;

// The static assets we need to load in app.
pub static ASSETS: Assets = Assets;

// Implement the AssetProvider trait here (required by App::new).
impl AssetProvider for Assets {
    fn get(&self, path: &str) -> Result<Cow<'_, [u8]>> {
        <Assets as RustEmbed>::get(path)
            .map(|f| f.data)
            .ok_or_else(|| anyhow!("no asset exists at path {}", path))
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let callbacks = platform::AppCallbacks {
        on_should_close_window: Some(Box::new(|_, _| std::process::exit(0))),
        on_should_terminate_app: Some(Box::new(|_| std::process::exit(0))),
        ..Default::default()
    };
    
    let app_builder = platform::AppBuilder::new(callbacks, Box::new(ASSETS), None);
    let _ = app_builder.run(move |ctx| {
        ctx.add_window(
            warpui::AddWindowOptions::default(),
            root_view::RootView::new,
        );
    });

    Ok(())
}
