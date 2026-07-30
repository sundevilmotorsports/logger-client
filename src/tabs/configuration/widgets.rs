use gpui::{AnyElement, IntoElement, div, prelude::*, px};

use crate::theme;

pub(super) fn section_header(title: &str, add_btn: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(div().text_color(theme::fg()).child(title.to_string()))
        .child(add_btn)
        .into_any_element()
}

pub(super) fn row_container() -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .px(px(8.))
        .py(px(4.))
        .rounded_sm()
        .bg(theme::panel_bg())
}

pub(super) fn field_label(text: &str) -> AnyElement {
    div()
        .text_color(theme::muted())
        .child(text.to_string())
        .into_any_element()
}
