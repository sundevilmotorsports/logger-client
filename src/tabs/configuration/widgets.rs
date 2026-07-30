use gpui::{AnyElement, Context, FocusHandle, IntoElement, KeyDownEvent, div, prelude::*, px};

use crate::root_view::RootView;
use crate::theme;

use super::ConfigurationTab;

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

pub(super) fn text_field(
    id: impl Into<gpui::ElementId>,
    value: &str,
    width: f32,
    focus: &FocusHandle,
    get_mut: impl Fn(&mut ConfigurationTab) -> &mut String + 'static,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let focus_for_click = focus.clone();
    div()
        .id(id)
        .track_focus(focus)
        .on_click(move |_, window, _| focus_for_click.focus(window))
        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
            let ks = &event.keystroke;
            if ks.modifiers.platform || ks.modifiers.control || ks.modifiers.alt {
                return; // don't eat modifier shortcuts
            }
            let buf = get_mut(&mut this.configuration_tab);
            if ks.key == "backspace" {
                buf.pop();
            } else if let Some(ch) = &ks.key_char {
                buf.push_str(ch);
            }
            cx.notify();
        }))
        .w(px(width))
        .px(px(6.))
        .py(px(2.))
        .border_1()
        .border_color(theme::border())
        .rounded_sm()
        .cursor_text()
        .hover(|s| s.border_color(theme::accent()))
        .text_color(theme::fg())
        .child(if value.is_empty() {
            " ".to_string()
        } else {
            value.to_string()
        })
        .into_any_element()
}

pub(super) fn checkbox(
    id: impl Into<gpui::ElementId>,
    checked: bool,
    cx: &mut Context<RootView>,
    on_toggle: impl Fn(&mut RootView, &mut Context<RootView>) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .w(px(14.))
        .h(px(14.))
        .rounded_sm()
        .border_1()
        .border_color(theme::border())
        .bg(if checked {
            theme::accent()
        } else {
            theme::panel_bg()
        })
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            on_toggle(this, cx);
            cx.notify();
        }))
        .into_any_element()
}
