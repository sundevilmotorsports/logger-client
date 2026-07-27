use gpui::{
    Div, ElementId, Font, FontFallbacks, Hsla, SharedString, Stateful, div, prelude::*, px, rgb,
};

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
pub fn panel_bg() -> Hsla {
    rgb(0x141418).into()
}
pub fn titlebar_bg() -> Hsla {
    rgb(0x16161a).into()
}
pub fn border() -> Hsla {
    rgb(0x26262e).into()
}
pub fn fg() -> Hsla {
    rgb(0xd2d2dc).into()
}
pub fn muted() -> Hsla {
    rgb(0x6b6b7b).into()
}
pub fn accent() -> Hsla {
    rgb(0x6a9fd8).into()
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

/// Bordered chip button without a hover style
pub fn button_base(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(8.))
        .py(px(2.))
        .rounded_sm()
        .border_1()
        .border_color(border())
        .text_color(muted())
        .cursor_pointer()
        .child(label.into())
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    button_base(id, label).hover(|s| s.text_color(fg()).border_color(muted()).bg(panel_bg()))
}

/// Shared card container for tab content.
pub fn panel() -> Div {
    div()
        .bg(panel_bg())
        .border_1()
        .border_color(border())
        .rounded_md()
        .p(px(16.))
}

pub const FONT_SIZE: f32 = 13.;
pub const LABEL_COL: usize = 10;

pub const TITLEBAR_HEIGHT: f32 = 32.;
pub const TITLEBAR_LEFT_INSET: f32 = 24.;
pub const TITLEBAR_RIGHT_INSET: f32 = 24.;
