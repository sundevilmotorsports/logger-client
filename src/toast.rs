use gpui::{IntoElement, div, prelude::*, px};

use crate::theme;

#[derive(Clone, Copy)]
pub enum ToastKind {
    Success,
    Error,
}

pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
}

pub fn render(toasts: &[(u64, Toast)]) -> impl IntoElement {
    let mut stack = div()
        .absolute()
        .bottom(px(16.))
        .right(px(16.))
        .flex()
        .flex_col()
        .gap(px(8.));

    for (_, toast) in toasts {
        let color = match toast.kind {
            ToastKind::Success => theme::green(),
            ToastKind::Error => theme::red(),
        };
        stack = stack.child(
            div()
                .font(theme::mono_font())
                .text_size(px(theme::FONT_SIZE))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .px(px(12.))
                .py(px(8.))
                .max_w(px(320.))
                .rounded_md()
                .border_1()
                .border_color(color)
                .bg(theme::panel_bg())
                .shadow_md()
                .child(div().text_color(color).child("●"))
                .child(div().text_color(theme::fg()).child(toast.message.clone())),
        );
    }

    stack
}
