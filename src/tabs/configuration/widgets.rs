use gpui::{AnyElement, IntoElement, div, prelude::*, px};
use gpui_component::label::Label;

use crate::theme;

pub(super) fn section_header(title: &str, add_btn: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(Label::new(title.to_string()).text_color(theme::fg()))
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
    Label::new(text.to_string())
        .text_color(theme::muted())
        .into_any_element()
}
