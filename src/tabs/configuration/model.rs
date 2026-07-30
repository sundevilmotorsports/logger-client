use gpui::{Context, FocusHandle};

use crate::root_view::RootView;

pub(super) struct CanDeviceForm {
    pub(super) id: String,
    pub(super) extended: bool,
    pub(super) fd: bool,
    pub(super) signals: serde_json::Value,
    pub(super) id_focus: FocusHandle,
}

impl CanDeviceForm {
    pub(super) fn new(cx: &mut Context<RootView>) -> Self {
        Self {
            id: "0".to_string(),
            extended: false,
            fd: false,
            signals: serde_json::json!({"Fixed": []}),
            id_focus: cx.focus_handle(),
        }
    }
}

pub(super) struct AdcChannelForm {
    pub(super) name: String,
    pub(super) channel: String,
    pub(super) scale: String,
    pub(super) offset: String,
    pub(super) name_focus: FocusHandle,
    pub(super) channel_focus: FocusHandle,
    pub(super) scale_focus: FocusHandle,
    pub(super) offset_focus: FocusHandle,
}

impl AdcChannelForm {
    pub(super) fn new(cx: &mut Context<RootView>) -> Self {
        Self {
            name: String::new(),
            channel: "0".to_string(),
            scale: String::new(),
            offset: String::new(),
            name_focus: cx.focus_handle(),
            channel_focus: cx.focus_handle(),
            scale_focus: cx.focus_handle(),
            offset_focus: cx.focus_handle(),
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
    cx: &mut Context<RootView>,
) -> Vec<CanDeviceForm> {
    v.get("can_devices")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|d| CanDeviceForm {
                    id: d
                        .get("id")
                        .and_then(|v| v.as_u64())
                        .map(|n| format!("0x{n:X}"))
                        .unwrap_or_default(),
                    extended: d.get("extended").and_then(|v| v.as_bool()).unwrap_or(false),
                    fd: d.get("fd").and_then(|v| v.as_bool()).unwrap_or(false),
                    signals: d
                        .get("signals")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({"Fixed": []})),
                    id_focus: cx.focus_handle(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn adc_channels_from_json(
    v: &serde_json::Value,
    cx: &mut Context<RootView>,
) -> Vec<AdcChannelForm> {
    v.get("adc_channels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| AdcChannelForm {
                    name: c
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    channel: c
                        .get("channel")
                        .and_then(|v| v.as_u64())
                        .map(|n| n.to_string())
                        .unwrap_or_default(),
                    scale: c
                        .get("scale")
                        .and_then(|v| v.as_f64())
                        .map(|n| (n as f32).to_string())
                        .unwrap_or_default(),
                    offset: c
                        .get("offset")
                        .and_then(|v| v.as_f64())
                        .map(|n| (n as f32).to_string())
                        .unwrap_or_default(),
                    name_focus: cx.focus_handle(),
                    channel_focus: cx.focus_handle(),
                    scale_focus: cx.focus_handle(),
                    offset_focus: cx.focus_handle(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Validates and serializes the current form state into JSON
pub(super) fn build_config_json(
    tab: &super::ConfigurationTab,
) -> anyhow::Result<serde_json::Value> {
    let mut can_devices = Vec::with_capacity(tab.can_devices.len());
    for (i, d) in tab.can_devices.iter().enumerate() {
        let id = parse_can_id(&d.id)
            .ok_or_else(|| anyhow::anyhow!("CAN device {}: invalid id \"{}\"", i + 1, d.id))?;
        can_devices.push(serde_json::json!({
            "id": id,
            "extended": d.extended,
            "fd": d.fd,
            "signals": d.signals,
        }));
    }

    let mut adc_channels = Vec::with_capacity(tab.adc_channels.len());
    for (i, c) in tab.adc_channels.iter().enumerate() {
        let channel: u8 = c.channel.trim().parse().map_err(|_| {
            anyhow::anyhow!("ADC channel {}: invalid channel \"{}\"", i + 1, c.channel)
        })?;
        let scale = if c.scale.trim().is_empty() {
            None
        } else {
            Some(c.scale.trim().parse::<f32>().map_err(|_| {
                anyhow::anyhow!("ADC channel {}: invalid scale \"{}\"", i + 1, c.scale)
            })?)
        };
        let offset: f32 = if c.offset.trim().is_empty() {
            0.0
        } else {
            c.offset.trim().parse().map_err(|_| {
                anyhow::anyhow!("ADC channel {}: invalid offset \"{}\"", i + 1, c.offset)
            })?
        };
        adc_channels.push(serde_json::json!({
            "name": c.name,
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
