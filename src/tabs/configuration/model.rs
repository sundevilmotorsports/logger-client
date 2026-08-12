use gpui::{App, AppContext as _, Context, Entity, Window};
use gpui_component::input::InputState;

use crate::root_view::RootView;

pub(super) struct CanDeviceForm {
    pub(super) id: Entity<InputState>,
    pub(super) extended: bool,
    pub(super) fd: bool,
    pub(super) signals: SignalsForm,
    pub(super) expanded: bool,
}

impl CanDeviceForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<RootView>) -> Self {
        Self {
            id: cx.new(|cx| InputState::new(window, cx).default_value("0")),
            extended: false,
            fd: false,
            signals: SignalsForm::Fixed(Vec::new()),
            expanded: false,
        }
    }
}

pub(super) struct SignalForm {
    pub(super) name: Entity<InputState>,
    pub(super) start: Entity<InputState>,
    pub(super) len: Entity<InputState>,
    pub(super) signed: bool,
    pub(super) big_endian: bool,
    /// Empty means "no scale" (raw bytes logged), matching `Signal::scale: Option<f32>`.
    pub(super) scale: Entity<InputState>,
    pub(super) offset: Entity<InputState>,
}

impl SignalForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<RootView>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx)),
            start: cx.new(|cx| InputState::new(window, cx).default_value("0")),
            len: cx.new(|cx| InputState::new(window, cx).default_value("1")),
            signed: false,
            big_endian: false,
            scale: cx.new(|cx| InputState::new(window, cx)),
            offset: cx.new(|cx| InputState::new(window, cx)),
        }
    }

    fn from_json(v: &serde_json::Value, window: &mut Window, cx: &mut Context<RootView>) -> Self {
        let name = v
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let start = v
            .get("start")
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())
            .unwrap_or_else(|| "0".to_string());
        let len = v
            .get("len")
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())
            .unwrap_or_else(|| "1".to_string());
        let scale = v
            .get("scale")
            .and_then(|v| v.as_f64())
            .map(|n| (n as f32).to_string())
            .unwrap_or_default();
        let offset = v
            .get("offset")
            .and_then(|v| v.as_f64())
            .map(|n| (n as f32).to_string())
            .unwrap_or_default();
        Self {
            name: cx.new(|cx| InputState::new(window, cx).default_value(name)),
            start: cx.new(|cx| InputState::new(window, cx).default_value(start)),
            len: cx.new(|cx| InputState::new(window, cx).default_value(len)),
            signed: v.get("signed").and_then(|v| v.as_bool()).unwrap_or(false),
            big_endian: v
                .get("big_endian")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            scale: cx.new(|cx| InputState::new(window, cx).default_value(scale)),
            offset: cx.new(|cx| InputState::new(window, cx).default_value(offset)),
        }
    }

    fn to_json(&self, cx: &App, label: &str) -> anyhow::Result<serde_json::Value> {
        let name = self.name.read(cx).value().to_string();
        let start_text = self.start.read(cx).value().to_string();
        let start: usize = start_text
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("{label}: invalid start \"{start_text}\""))?;
        let len_text = self.len.read(cx).value().to_string();
        let len: usize = len_text
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("{label}: invalid len \"{len_text}\""))?;
        let scale_text = self.scale.read(cx).value().to_string();
        let scale = if scale_text.trim().is_empty() {
            None
        } else {
            Some(scale_text.trim().parse::<f32>().map_err(|_| {
                anyhow::anyhow!("{label}: invalid scale \"{scale_text}\"")
            })?)
        };
        let offset_text = self.offset.read(cx).value().to_string();
        let offset: f32 = if offset_text.trim().is_empty() {
            0.0
        } else {
            offset_text
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("{label}: invalid offset \"{offset_text}\""))?
        };
        Ok(serde_json::json!({
            "name": name,
            "start": start,
            "len": len,
            "signed": self.signed,
            "big_endian": self.big_endian,
            "scale": scale,
            "offset": offset,
        }))
    }
}

pub(super) struct SignalGroupForm {
    pub(super) type_val: Entity<InputState>,
    pub(super) signals: Vec<SignalForm>,
}

pub(super) enum SignalsForm {
    Fixed(Vec<SignalForm>),
    Muxed {
        byte: Entity<InputState>,
        groups: Vec<SignalGroupForm>,
    },
}

impl SignalsForm {
    pub(super) fn empty_muxed(window: &mut Window, cx: &mut Context<RootView>) -> Self {
        SignalsForm::Muxed {
            byte: cx.new(|cx| InputState::new(window, cx).default_value("0")),
            groups: Vec::new(),
        }
    }

    fn to_json(&self, cx: &App, label: &str) -> anyhow::Result<serde_json::Value> {
        match self {
            SignalsForm::Fixed(sigs) => {
                let sigs = sigs
                    .iter()
                    .enumerate()
                    .map(|(i, s)| s.to_json(cx, &format!("{label} signal {}", i + 1)))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(serde_json::json!({ "Fixed": sigs }))
            }
            SignalsForm::Muxed { byte, groups } => {
                let byte_text = byte.read(cx).value().to_string();
                let byte_val: usize = byte_text
                    .trim()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("{label}: invalid mux byte \"{byte_text}\""))?;
                let groups = groups
                    .iter()
                    .enumerate()
                    .map(|(gi, g)| {
                        let type_text = g.type_val.read(cx).value().to_string();
                        let type_val: u8 = type_text.trim().parse().map_err(|_| {
                            anyhow::anyhow!(
                                "{label} group {}: invalid type_val \"{type_text}\"",
                                gi + 1
                            )
                        })?;
                        let signals = g
                            .signals
                            .iter()
                            .enumerate()
                            .map(|(i, s)| {
                                s.to_json(cx, &format!("{label} group {} signal {}", gi + 1, i + 1))
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        Ok(serde_json::json!({ "type_val": type_val, "signals": signals }))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(serde_json::json!({ "Muxed": { "byte": byte_val, "groups": groups } }))
            }
        }
    }
}

fn signals_from_json(
    v: &serde_json::Value,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> SignalsForm {
    if let Some(arr) = v.get("Fixed").and_then(|v| v.as_array()) {
        return SignalsForm::Fixed(
            arr.iter()
                .map(|s| SignalForm::from_json(s, window, cx))
                .collect(),
        );
    }
    if let Some(m) = v.get("Muxed") {
        let byte = m
            .get("byte")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            .to_string();
        let groups = m
            .get("groups")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|g| {
                        let type_val = g
                            .get("type_val")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                            .to_string();
                        let signals = g
                            .get("signals")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .map(|s| SignalForm::from_json(s, window, cx))
                                    .collect()
                            })
                            .unwrap_or_default();
                        SignalGroupForm {
                            type_val: cx
                                .new(|cx| InputState::new(window, cx).default_value(type_val)),
                            signals,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        return SignalsForm::Muxed {
            byte: cx.new(|cx| InputState::new(window, cx).default_value(byte)),
            groups,
        };
    }
    SignalsForm::Fixed(Vec::new())
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

pub(super) fn signal_count(s: &SignalsForm) -> usize {
    match s {
        SignalsForm::Fixed(sigs) => sigs.len(),
        SignalsForm::Muxed { groups, .. } => groups.iter().map(|g| g.signals.len()).sum(),
    }
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
                            .map(|s| signals_from_json(s, window, cx))
                            .unwrap_or_else(|| SignalsForm::Fixed(Vec::new())),
                        expanded: false,
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
        let signals = d.signals.to_json(cx, &format!("CAN device {}", i + 1))?;
        can_devices.push(serde_json::json!({
            "id": id,
            "extended": d.extended,
            "fd": d.fd,
            "signals": signals,
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
