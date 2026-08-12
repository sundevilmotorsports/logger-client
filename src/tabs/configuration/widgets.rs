use gpui::{AnyElement, IntoElement, prelude::*, px};
use gpui_component::h_flex;
use gpui_component::label::Label;

use crate::theme;

pub(super) fn section_header(title: &str, add_btn: impl IntoElement) -> AnyElement {
    h_flex()
        .justify_between()
        .child(Label::new(title.to_string()).text_color(theme::fg()))
        .child(add_btn)
        .into_any_element()
}

pub(super) fn row_container() -> gpui::Div {
    h_flex()
        .flex_wrap()
        .gap(px(8.))
        .px(px(8.))
        .py(px(4.))
        .rounded_sm()
        .bg(theme::panel_bg())
}

pub(super) fn indented_row() -> gpui::Div {
    row_container().ml(px(20.))
}

pub(super) fn field_label(text: &str) -> AnyElement {
    Label::new(text.to_string())
        .text_color(theme::muted())
        .into_any_element()
}
