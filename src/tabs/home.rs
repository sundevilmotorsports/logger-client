use slint::ComponentHandle;

use super::{kv_row, span};
use crate::app::model;
use crate::device::{DeviceState, DeviceStatusFlags, Resources};
use crate::ui::{AppWindow, HomeState, Span, Tone};

fn format_uptime(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn format_kb(bytes: u32) -> String {
    format!("{} KB", bytes / 1024)
}

fn resources_spans(resources: Option<&Resources>) -> Vec<Span> {
    let Some(r) = resources else {
        return vec![span("—", Tone::Muted)];
    };

    let mut spans = vec![span(
        format!(
            "heap {} (min {})",
            format_kb(r.heap_free),
            format_kb(r.heap_min_free)
        ),
        Tone::Fg,
    )];
    for (i, load) in r.cpu_load.iter().enumerate() {
        let tone = match load {
            l if *l >= 90.0 => Tone::Red,
            l if *l >= 70.0 => Tone::Amber,
            _ => Tone::Green,
        };
        spans.push(span(format!("core{i} {load:.0}%"), tone));
    }
    spans
}

fn health_spans(status: Option<DeviceStatusFlags>) -> Vec<Span> {
    let items: [(&str, Option<bool>); 7] = [
        ("adc", status.map(|s| s.adc)),
        ("can", status.map(|s| s.can)),
        ("gnss", status.map(|s| s.gnss)),
        ("power", status.map(|s| s.power)),
        ("logging", status.map(|s| s.logging)),
        ("sd", status.map(|s| s.sd)),
        ("serial", status.map(|s| s.serial)),
    ];

    items
        .into_iter()
        .map(|(label, ok)| {
            let tone = match ok {
                Some(true) => Tone::Green,
                Some(false) => Tone::Red,
                None => Tone::Muted,
            };
            span(label, tone)
        })
        .collect()
}

pub fn render(state: &DeviceState, ui: &AppWindow) {
    let status = match &state.port {
        Some(_) => span("connected", Tone::Green),
        None => span("scanning...", Tone::Amber),
    };
    let port = state.port.as_deref().unwrap_or("—");
    let uptime = state
        .uptime
        .map(format_uptime)
        .unwrap_or_else(|| "—".to_string());

    let logging = match state.logging_active {
        Some(true) => span("recording", Tone::Green),
        Some(false) => span("paused", Tone::Amber),
        None => span("—", Tone::Muted),
    };
    let current_log = state.current_log.as_deref().unwrap_or("—");

    let gps = match &state.gps {
        Some(f) => span(
            format!(
                "{:.5}, {:.5}  {:.0}m  {} sats",
                f.lat, f.lon, f.alt_m, f.sats
            ),
            Tone::Green,
        ),
        None => span("no fix", Tone::Muted),
    };

    let power = match &state.power {
        Some(p) => span(
            format!("{:.0} mA, {:.0} mV", p.current_ma(), p.voltage_mv()),
            Tone::Green,
        ),
        None => span("no reading", Tone::Muted),
    };

    let rows = vec![
        kv_row("status", vec![status]),
        kv_row("port", vec![span(port, Tone::Fg)]),
        kv_row("uptime", vec![span(uptime, Tone::Fg)]),
        kv_row("health", health_spans(state.status)),
        kv_row("resources", resources_spans(state.resources.as_ref())),
        kv_row("logging", vec![logging]),
        kv_row("file", vec![span(current_log, Tone::Fg)]),
        kv_row("gps", vec![gps]),
        kv_row("power", vec![power]),
    ];
    ui.global::<HomeState>().set_rows(model(rows));
}
