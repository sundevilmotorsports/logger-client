use pathfinder_color::ColorU;
use warpui::{
    Element,
    elements::{
        CrossAxisAlignment, Flex, Hoverable, MainAxisAlignment, MouseStateHandle, ParentElement,
        Text,
    },
    fonts::FamilyId,
    platform::Cursor,
};

use crate::device::{self, DeviceState};
use crate::theme::{AMBER, FG, FONT_SIZE, GREEN, LABEL_COL, MUTED, RED};

fn format_uptime(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[derive(Default)]
pub struct HomeTab {
    ping_hover: MouseStateHandle,
}

impl HomeTab {
    fn row(&self, font: FamilyId, label: &str, value: &str, color: ColorU) -> Box<dyn Element> {
        let padded = format!("{label:<LABEL_COL$}");
        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .with_child(
                Text::new_inline(padded, font, FONT_SIZE)
                    .with_color(MUTED)
                    .finish(),
            )
            .with_child(
                Text::new_inline(value.to_string(), font, FONT_SIZE)
                    .with_color(color)
                    .finish(),
            )
            .finish()
    }

    pub fn render(
        &self,
        state: &DeviceState,
        font: FamilyId,
        req_tx: &device::RequestTx,
    ) -> Box<dyn Element> {
        let (status, status_color) = match &state.port {
            Some(_) => ("connected", GREEN),
            None => ("scanning...", AMBER),
        };
        let port = state.port.as_deref().unwrap_or("—");
        let uptime = state
            .uptime
            .map(format_uptime)
            .unwrap_or_else(|| "—".to_string());
        let (gps_str, gps_color) = match &state.gps {
            Some(f) => (
                format!("{:.5}, {:.5}  {:.0}m  {} sats", f.lat, f.lon, f.alt_m, f.sats),
                GREEN,
            ),
            None => ("no fix".to_string(), MUTED),
        };
        let (ping_str, ping_color) = match state.last_ping {
            Some(true) => ("ok", GREEN),
            Some(false) => ("error", RED),
            None => ("—", MUTED),
        };

        let req_tx = req_tx.clone();

        let ping_btn = Hoverable::new(self.ping_hover.clone(), move |ms| {
            let color = if ms.is_hovered() { FG } else { MUTED };
            Text::new_inline("[ ping ]", font, FONT_SIZE)
                .with_color(color)
                .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |_, _, _| {
            let _ = req_tx.send(device::Command::Ping);
        })
        .finish();

        Flex::column()
            .with_spacing(4.)
            .with_child(self.row(font, "status", status, status_color))
            .with_child(self.row(font, "port", port, FG))
            .with_child(self.row(font, "uptime", &uptime, FG))
            .with_child(self.row(font, "gps", &gps_str, gps_color))
            .with_child(
                Text::new_inline("", font, FONT_SIZE)
                    .with_color(MUTED)
                    .finish(),
            )
            .with_child(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(8.)
                    .with_child(ping_btn)
                    .with_child(
                        Text::new_inline(ping_str, font, FONT_SIZE)
                            .with_color(ping_color)
                            .finish(),
                    )
                    .finish(),
            )
            .finish()
    }
}
