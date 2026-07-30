use gpui::{AnyElement, Hsla, div, prelude::*, px};
use gpui_component::Sizable;
use gpui_component::description_list::DescriptionList;
use gpui_component::label::Label;

use crate::device::{self, DeviceState};
use crate::theme::{self, FONT_SIZE};

fn format_uptime(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn value(text: String, color: Hsla) -> AnyElement {
    Label::new(text).text_color(color).into_any_element()
}

#[derive(Default)]
pub struct HomeTab;

impl HomeTab {
    pub fn render(&self, state: &DeviceState, _req_tx: &device::RequestTx) -> AnyElement {
        let (status, status_color) = match &state.port {
            Some(_) => ("connected".to_string(), theme::green()),
            None => ("scanning...".to_string(), theme::amber()),
        };
        let port = state.port.clone().unwrap_or_else(|| "—".to_string());
        let uptime = state
            .uptime
            .map(format_uptime)
            .unwrap_or_else(|| "—".to_string());
        let (gps_str, gps_color) = match &state.gps {
            Some(f) => (
                format!(
                    "{:.5}, {:.5}  {:.0}m  {} sats",
                    f.lat, f.lon, f.alt_m, f.sats
                ),
                theme::green(),
            ),
            None => ("no fix".to_string(), theme::muted()),
        };

        let (imu_str, imu_color) = match &state.imu {
            Some(i) => (
                format!(
                    "{:.2}x {:.2}y {:.2}z, {:.1}c, mag {:.1}/{:.1}/{:.1}uT",
                    i.accel_g[0],
                    i.accel_g[1],
                    i.accel_g[2],
                    i.temp_c,
                    i.mag_ut[0],
                    i.mag_ut[1],
                    i.mag_ut[2]
                ),
                theme::green(),
            ),
            None => ("no imu".to_string(), theme::muted()),
        };

        div()
            .font(theme::mono_font())
            .text_size(px(FONT_SIZE))
            .child(
                DescriptionList::horizontal()
                    .small()
                    .columns(1)
                    .bordered(true)
                    .label_width(px(90.))
                    .item("status", value(status, status_color), 1)
                    .item("port", value(port, theme::fg()), 1)
                    .item("uptime", value(uptime, theme::fg()), 1)
                    .item("gps", value(gps_str, gps_color), 1)
                    .item("imu", value(imu_str, imu_color), 1),
            )
            .into_any_element()
    }
}
