use gpui::{AnyElement, Hsla, div, prelude::*};

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

#[derive(Default)]
pub struct HomeTab;

impl HomeTab {
    fn row(&self, label: &str, value: &str, color: Hsla) -> AnyElement {
        let padded = format!("{:<width$}", label, width = theme::LABEL_COL);
        div()
            .flex()
            .flex_row()
            .items_center()
            .child(
                div()
                    .text_color(theme::muted())
                    .child(padded)
                    .whitespace_nowrap(),
            )
            .child(div().text_color(color).child(value.to_string()))
            .into_any_element()
    }

    pub fn render(&self, state: &DeviceState, req_tx: &device::RequestTx) -> AnyElement {
        let (status, status_color) = match &state.port {
            Some(_) => ("connected", theme::green()),
            None => ("scanning...", theme::amber()),
        };
        let port = state.port.as_deref().unwrap_or("—");
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

        let (ping_str, ping_color) = match state.last_ping {
            Some(true) => ("ok", theme::green()),
            Some(false) => ("error", theme::red()),
            None => ("—", theme::muted()),
        };

        let req_tx = req_tx.clone();

        let ping_btn = div()
            .id("ping-button")
            .text_color(theme::muted())
            .hover(|s| s.text_color(theme::fg()))
            .cursor_pointer()
            .child("[ ping ]")
            .on_click(move |_, _, _| {
                let _ = req_tx.send(device::Command::Ping);
            });

        div()
            .font(theme::mono_font())
            .text_size(gpui::px(FONT_SIZE))
            .flex()
            .flex_col()
            .gap(gpui::px(4.))
            .child(self.row("status", status, status_color))
            .child(self.row("port", port, theme::fg()))
            .child(self.row("uptime", &uptime, theme::fg()))
            .child(self.row("gps", &gps_str, gps_color))
            .child(self.row("imu", &imu_str, imu_color))
            .child(div().h(gpui::px(FONT_SIZE)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(gpui::px(8.))
                    .child(ping_btn)
                    .child(div().text_color(ping_color).child(ping_str)),
            )
            .into_any_element()
    }
}
