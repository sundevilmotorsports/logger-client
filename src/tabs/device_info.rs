use gpui::{AnyElement, IntoElement, div, prelude::*};

use crate::device::DeviceState;
use crate::theme::{self, FONT_SIZE};

#[derive(Default)]
pub struct DeviceInfoTab;

impl DeviceInfoTab {
    pub fn render(&self, state: &DeviceState) -> AnyElement {
        let version = state.firmware_version.as_deref().unwrap_or("—");
        let padded = format!("{:<width$}", "firmware", width = theme::LABEL_COL);

        div()
            .font(theme::mono_font())
            .text_size(gpui::px(FONT_SIZE))
            .flex()
            .flex_row()
            .items_center()
            .child(
                div()
                    .text_color(theme::muted())
                    .child(padded)
                    .whitespace_nowrap(),
            )
            .child(div().text_color(theme::fg()).child(version.to_string()))
            .into_any_element()
    }
}
