use gpui::{App, Font, FontFallbacks, Hsla, px, rgb};
use gpui_component::theme::Theme;

pub fn apply(cx: &mut App) {
    fn shift(c: Hsla, dl: f32) -> Hsla {
        Hsla {
            l: (c.l + dl).clamp(0., 1.),
            ..c
        }
    }

    let theme = Theme::global_mut(cx);
    theme.font_family = "JetBrainsMono Nerd Font".into();
    // `font_size` becomes the window rem size, and components render their
    // text at text_sm (0.875rem)
    theme.font_size = px(FONT_SIZE / 0.875);

    let c = &mut theme.colors;
    c.background = bg();
    c.foreground = fg();
    c.border = border();
    c.input = border();
    c.ring = accent();
    c.caret = fg();

    c.primary = accent();
    c.primary_hover = shift(accent(), 0.06);
    c.primary_active = shift(accent(), -0.06);
    c.primary_foreground = bg();

    c.secondary = rgb(0x1e1e24).into();
    c.secondary_hover = rgb(0x26262e).into();
    c.secondary_active = rgb(0x2e2e38).into();
    c.secondary_foreground = fg();

    c.danger = red();
    c.danger_hover = shift(red(), 0.06);
    c.danger_active = shift(red(), -0.06);
    c.danger_foreground = fg();

    c.accent = rgb(0x22222a).into();
    c.accent_foreground = fg();
    c.muted = panel_bg();
    c.muted_foreground = muted();
    c.popover = panel_bg();
    c.popover_foreground = fg();
    c.title_bar = titlebar_bg();
    c.success = green();
    c.warning = amber();
    c.selection = Hsla { a: 0.25, ..accent() };

    c.tab_foreground = muted();
    c.tab_active_foreground = fg();
    c.progress_bar = green();
    c.description_list_label = panel_bg();
    c.description_list_label_foreground = muted();
}

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

pub const FONT_SIZE: f32 = 13.;

pub const TITLEBAR_HEIGHT: f32 = 32.;
pub const TITLEBAR_LEFT_INSET: f32 = 24.;
pub const TITLEBAR_RIGHT_INSET: f32 = 24.;
