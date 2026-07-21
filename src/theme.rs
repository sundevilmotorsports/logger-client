use gpui::{Font, FontFallbacks, Hsla, rgb};

pub fn mono_font() -> Font {
    Font {
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "Hack".into(),
            "FreeMono".into(),
            "Menlo".into(),
            "Courier New".into(),
        ])),
        ..gpui::font("JetBrainsMono Nerd Font")
    }
}

pub fn bg() -> Hsla {
    rgb(0x0d0d0f).into()
}
pub fn titlebar_bg() -> Hsla {
    rgb(0x16161a).into()
}
pub fn fg() -> Hsla {
    rgb(0xc8c8d2).into()
}
pub fn muted() -> Hsla {
    rgb(0x50505f).into()
}
pub fn green() -> Hsla {
    rgb(0x50c878).into()
}
pub fn amber() -> Hsla {
    rgb(0xc8a03c).into()
}
pub fn red() -> Hsla {
    rgb(0xc85050).into()
}

pub const FONT_SIZE: f32 = 13.;
pub const LABEL_COL: usize = 10;

pub const TITLEBAR_HEIGHT: f32 = 38.;
pub const TITLEBAR_LEFT_INSET: f32 = 78.;
