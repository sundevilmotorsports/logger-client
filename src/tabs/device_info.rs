use gpui::{AnyElement, div, prelude::*, px};
use gpui_component::Sizable;
use gpui_component::description_list::DescriptionList;

use crate::device::DeviceState;
use crate::theme::{self, FONT_SIZE};

#[derive(Default)]
pub struct DeviceInfoTab;

impl DeviceInfoTab {
    pub fn render(&self, state: &DeviceState) -> AnyElement {
        let version = state.firmware_version.clone().unwrap_or_else(|| "—".into());

        div()
            .font(theme::mono_font())
            .text_size(px(FONT_SIZE))
            .child(
                DescriptionList::horizontal()
                    .small()
                    .columns(1)
                    .bordered(true)
                    .label_width(px(90.))
                    .item("firmware", version, 1),
            )
            .into_any_element()
    }
}
