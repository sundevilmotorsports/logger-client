use slint::ComponentHandle;

use super::{kv_row, span};
use crate::app::model;
use crate::device::DeviceState;
use crate::ui::{AppWindow, InfoState, Tone};

pub fn render(state: &DeviceState, ui: &AppWindow) {
    let version = state.firmware_version.as_deref().unwrap_or("—");
    let rows = vec![kv_row("firmware", vec![span(version, Tone::Fg)])];
    ui.global::<InfoState>().set_rows(model(rows));
}
