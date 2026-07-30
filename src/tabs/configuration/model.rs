use gpui::{App, AppContext as _, Context, Entity, Window};
use gpui_component::input::InputState;

use crate::root_view::RootView;

pub(super) struct CanDeviceForm {
    pub(super) id: Entity<InputState>,
    pub(super) extended: bool,
    pub(super) fd: bool,
    pub(super) signals: serde_json::Value,
}

impl CanDeviceForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<RootView>) -> Self {
        Self {
            id: cx.new(|cx| InputState::new(window, cx).default_value("0")),
            extended: false,
            fd: false,
            signals: serde_json::json!({"Fixed": []}),
        }
    }
}

pub(super) struct AdcChannelForm {
    pub(super) name: Entity<InputState>,
    pub(super) channel: Entity<InputState>,
    /// Empty means "no scale" (raw counts logged), matching `AdcChannel::scale: Option<f32>`.
    pub(super) scale: Entity<InputState>,
    pub(super) offset: Entity<InputState>,
}

impl AdcChannelForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<RootView>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx)),
            channel: cx.new(|cx| InputState::new(window, cx).default_value("0")),
            scale: cx.new(|cx| InputState::new(window, cx)),
            offset: cx.new(|cx| InputState::new(window, cx)),
        }
    }
}

pub(super) fn signal_count(v: &serde_json::Value) -> usize {
    if let Some(arr) = v.get("Fixed").and_then(|v| v.as_array()) {
        return arr.len();
    }
    if let Some(groups) = v
        .get("Muxed")
        .and_then(|v| v.get("groups"))
        .and_then(|v| v.as_array())
    {
        return groups
            .iter()
            .filter_map(|g| g.get("signals").and_then(|s| s.as_array()).map(Vec::len))
            .sum();
    }
    0
}

fn parse_can_id(s: &str) -> Option<u32> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

pub(super) fn can_devices_from_json(
    v: &serde_json::Value,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Vec<CanDeviceForm> {
    v.get("can_devices")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|d| {
                    let id_text = d
                        .get("id")
                        .and_then(|v| v.as_u64())
                        .map(|n| format!("0x{n:X}"))
                        .unwrap_or_default();
                    CanDeviceForm {
                        id: cx.new(|cx| InputState::new(window, cx).default_value(id_text)),
                        extended: d.get("extended").and_then(|v| v.as_bool()).unwrap_or(false),
                        fd: d.get("fd").and_then(|v| v.as_bool()).unwrap_or(false),
                        signals: d
                            .get("signals")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({"Fixed": []})),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn adc_channels_from_json(
    v: &serde_json::Value,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Vec<AdcChannelForm> {
    v.get("adc_channels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| {
                    let name = c
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let channel = c
                        .get("channel")
                        .and_then(|v| v.as_u64())
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    let scale = c
                        .get("scale")
                        .and_then(|v| v.as_f64())
                        .map(|n| (n as f32).to_string())
                        .unwrap_or_default();
                    let offset = c
                        .get("offset")
                        .and_then(|v| v.as_f64())
                        .map(|n| (n as f32).to_string())
                        .unwrap_or_default();
                    AdcChannelForm {
                        name: cx.new(|cx| InputState::new(window, cx).default_value(name)),
                        channel: cx.new(|cx| InputState::new(window, cx).default_value(channel)),
                        scale: cx.new(|cx| InputState::new(window, cx).default_value(scale)),
                        offset: cx.new(|cx| InputState::new(window, cx).default_value(offset)),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Validates and serializes the current form state into JSON
pub(super) fn build_config_json(
    tab: &super::ConfigurationTab,
    cx: &App,
) -> anyhow::Result<serde_json::Value> {
    let mut can_devices = Vec::with_capacity(tab.can_devices.len());
    for (i, d) in tab.can_devices.iter().enumerate() {
        let id_text = d.id.read(cx).value().to_string();
        let id = parse_can_id(&id_text)
            .ok_or_else(|| anyhow::anyhow!("CAN device {}: invalid id \"{id_text}\"", i + 1))?;
        can_devices.push(serde_json::json!({
            "id": id,
            "extended": d.extended,
            "fd": d.fd,
            "signals": d.signals,
        }));
    }

    let mut adc_channels = Vec::with_capacity(tab.adc_channels.len());
    for (i, c) in tab.adc_channels.iter().enumerate() {
        let name = c.name.read(cx).value().to_string();
        let channel_text = c.channel.read(cx).value().to_string();
        let channel: u8 = channel_text.trim().parse().map_err(|_| {
            anyhow::anyhow!("ADC channel {}: invalid channel \"{channel_text}\"", i + 1)
        })?;
        let scale_text = c.scale.read(cx).value().to_string();
        let scale = if scale_text.trim().is_empty() {
            None
        } else {
            Some(scale_text.trim().parse::<f32>().map_err(|_| {
                anyhow::anyhow!("ADC channel {}: invalid scale \"{scale_text}\"", i + 1)
            })?)
        };
        let offset_text = c.offset.read(cx).value().to_string();
        let offset: f32 = if offset_text.trim().is_empty() {
            0.0
        } else {
            offset_text.trim().parse().map_err(|_| {
                anyhow::anyhow!("ADC channel {}: invalid offset \"{offset_text}\"", i + 1)
            })?
        };
        adc_channels.push(serde_json::json!({
            "name": name,
            "channel": channel,
            "scale": scale,
            "offset": offset,
        }));
    }

    Ok(serde_json::json!({
        "can_devices": can_devices,
        "adc_channels": adc_channels,
    }))
}
