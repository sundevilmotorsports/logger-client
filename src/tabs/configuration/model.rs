use crate::app::model;
use crate::ui::{AdcChannel, CanDevice, Signal, SignalGroup};

#[derive(Default)]
pub(super) struct CanDeviceForm {
    pub(super) id: String,
    pub(super) extended: bool,
    pub(super) fd: bool,
    pub(super) engine: bool,
    pub(super) signals: SignalsForm,
    pub(super) expanded: bool,
}

impl CanDeviceForm {
    pub(super) fn new() -> Self {
        Self {
            id: "0".to_string(),
            ..Default::default()
        }
    }

    pub(super) fn signal_count(&self) -> usize {
        match &self.signals {
            SignalsForm::Fixed(sigs) => sigs.len(),
            SignalsForm::Muxed { groups, .. } => groups.iter().map(|g| g.signals.len()).sum(),
        }
    }

    pub(super) fn to_ui(&self) -> CanDevice {
        let (muxed, mux_byte, signals, groups) = match &self.signals {
            SignalsForm::Fixed(sigs) => (false, "", signals_model(sigs), model(Vec::new())),
            SignalsForm::Muxed { byte, groups } => (
                true,
                byte.as_str(),
                model(Vec::new()),
                model(
                    groups
                        .iter()
                        .map(|g| SignalGroup {
                            type_val: g.type_val.as_str().into(),
                            signals: signals_model(&g.signals),
                        })
                        .collect(),
                ),
            ),
        };
        CanDevice {
            id: self.id.as_str().into(),
            extended: self.extended,
            fd: self.fd,
            engine: self.engine,
            expanded: self.expanded,
            signal_count: self.signal_count() as i32,
            muxed,
            mux_byte: mux_byte.into(),
            signals,
            groups,
        }
    }

    /// The signal list at `group` (`None` for fixed signals), if the device
    /// is in the matching mode.
    pub(super) fn signal_list_mut(&mut self, group: Option<usize>) -> Option<&mut Vec<SignalForm>> {
        match (&mut self.signals, group) {
            (SignalsForm::Fixed(sigs), None) => Some(sigs),
            (SignalsForm::Muxed { groups, .. }, Some(gi)) => {
                groups.get_mut(gi).map(|g| &mut g.signals)
            }
            _ => None,
        }
    }
}

fn signals_model(sigs: &[SignalForm]) -> slint::ModelRc<Signal> {
    model(sigs.iter().map(SignalForm::to_ui).collect())
}

pub(super) struct SignalForm {
    pub(super) name: String,
    pub(super) start: String,
    pub(super) len: String,
    pub(super) signed: bool,
    pub(super) big_endian: bool,
    /// Empty means "no scale" (raw bytes logged), matching `Signal::scale: Option<f32>`.
    pub(super) scale: String,
    pub(super) offset: String,
}

impl SignalForm {
    pub(super) fn new() -> Self {
        Self {
            name: String::new(),
            start: "0".to_string(),
            len: "1".to_string(),
            signed: false,
            big_endian: false,
            scale: String::new(),
            offset: String::new(),
        }
    }

    fn from_json(v: &serde_json::Value) -> Self {
        Self {
            name: v
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            start: v
                .get("start")
                .and_then(|v| v.as_u64())
                .map(|n| n.to_string())
                .unwrap_or_else(|| "0".to_string()),
            len: v
                .get("len")
                .and_then(|v| v.as_u64())
                .map(|n| n.to_string())
                .unwrap_or_else(|| "1".to_string()),
            signed: v.get("signed").and_then(|v| v.as_bool()).unwrap_or(false),
            big_endian: v
                .get("big_endian")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            scale: float_text(v.get("scale")),
            offset: float_text(v.get("offset")),
        }
    }

    fn to_ui(&self) -> Signal {
        Signal {
            name: self.name.as_str().into(),
            start: self.start.as_str().into(),
            len: self.len.as_str().into(),
            signed: self.signed,
            big_endian: self.big_endian,
            scale: self.scale.as_str().into(),
            offset: self.offset.as_str().into(),
        }
    }

    fn to_json(&self, label: &str) -> anyhow::Result<serde_json::Value> {
        let start: usize = self
            .start
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("{label}: invalid start \"{}\"", self.start))?;
        let len: usize = self
            .len
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("{label}: invalid len \"{}\"", self.len))?;
        let scale = if self.scale.trim().is_empty() {
            None
        } else {
            Some(
                self.scale
                    .trim()
                    .parse::<f32>()
                    .map_err(|_| anyhow::anyhow!("{label}: invalid scale \"{}\"", self.scale))?,
            )
        };
        let offset: f32 = if self.offset.trim().is_empty() {
            0.0
        } else {
            self.offset
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("{label}: invalid offset \"{}\"", self.offset))?
        };
        Ok(serde_json::json!({
            "name": self.name,
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
    pub(super) type_val: String,
    pub(super) signals: Vec<SignalForm>,
}

pub(super) enum SignalsForm {
    Fixed(Vec<SignalForm>),
    Muxed {
        byte: String,
        groups: Vec<SignalGroupForm>,
    },
}

impl Default for SignalsForm {
    fn default() -> Self {
        SignalsForm::Fixed(Vec::new())
    }
}

impl SignalsForm {
    pub(super) fn empty_muxed() -> Self {
        SignalsForm::Muxed {
            byte: "0".to_string(),
            groups: Vec::new(),
        }
    }

    fn to_json(&self, label: &str) -> anyhow::Result<serde_json::Value> {
        match self {
            SignalsForm::Fixed(sigs) => {
                let sigs = sigs
                    .iter()
                    .enumerate()
                    .map(|(i, s)| s.to_json(&format!("{label} signal {}", i + 1)))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(serde_json::json!({ "Fixed": sigs }))
            }
            SignalsForm::Muxed { byte, groups } => {
                let byte_val: usize = byte
                    .trim()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("{label}: invalid mux byte \"{byte}\""))?;
                let groups = groups
                    .iter()
                    .enumerate()
                    .map(|(gi, g)| {
                        let type_val: u8 = g.type_val.trim().parse().map_err(|_| {
                            anyhow::anyhow!(
                                "{label} group {}: invalid type_val \"{}\"",
                                gi + 1,
                                g.type_val
                            )
                        })?;
                        let signals = g
                            .signals
                            .iter()
                            .enumerate()
                            .map(|(i, s)| {
                                s.to_json(&format!("{label} group {} signal {}", gi + 1, i + 1))
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

fn signals_from_json(v: &serde_json::Value) -> SignalsForm {
    if let Some(arr) = v.get("Fixed").and_then(|v| v.as_array()) {
        return SignalsForm::Fixed(arr.iter().map(SignalForm::from_json).collect());
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
                    .map(|g| SignalGroupForm {
                        type_val: g
                            .get("type_val")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                            .to_string(),
                        signals: g
                            .get("signals")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().map(SignalForm::from_json).collect())
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        return SignalsForm::Muxed { byte, groups };
    }
    SignalsForm::default()
}

#[derive(Default)]
pub(super) struct AdcChannelForm {
    pub(super) name: String,
    pub(super) channel: String,
    /// Empty means "no scale" (raw counts logged), matching `AdcChannel::scale: Option<f32>`.
    pub(super) scale: String,
    pub(super) offset: String,
}

impl AdcChannelForm {
    pub(super) fn new() -> Self {
        Self {
            channel: "0".to_string(),
            ..Default::default()
        }
    }

    pub(super) fn to_ui(&self) -> AdcChannel {
        AdcChannel {
            name: self.name.as_str().into(),
            channel: self.channel.as_str().into(),
            scale: self.scale.as_str().into(),
            offset: self.offset.as_str().into(),
        }
    }
}

fn float_text(v: Option<&serde_json::Value>) -> String {
    v.and_then(|v| v.as_f64())
        .map(|n| (n as f32).to_string())
        .unwrap_or_default()
}

fn parse_can_id(s: &str) -> Option<u32> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

pub(super) fn can_devices_from_json(v: &serde_json::Value) -> Vec<CanDeviceForm> {
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
                    engine: d.get("bus").and_then(|v| v.as_str()) == Some("Engine"),
                    signals: d.get("signals").map(signals_from_json).unwrap_or_default(),
                    expanded: false,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn adc_channels_from_json(v: &serde_json::Value) -> Vec<AdcChannelForm> {
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
                    scale: float_text(c.get("scale")),
                    offset: float_text(c.get("offset")),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Validates and serializes the current form state into JSON
pub(super) fn build_config_json(tab: &super::ConfigurationTab) -> anyhow::Result<serde_json::Value> {
    let mut can_devices = Vec::with_capacity(tab.can_devices.len());
    for (i, d) in tab.can_devices.iter().enumerate() {
        let id = parse_can_id(&d.id)
            .ok_or_else(|| anyhow::anyhow!("CAN device {}: invalid id \"{}\"", i + 1, d.id))?;
        let signals = d.signals.to_json(&format!("CAN device {}", i + 1))?;
        can_devices.push(serde_json::json!({
            "id": id,
            "extended": d.extended,
            "fd": d.fd,
            "bus": if d.engine { "Engine" } else { "Module" },
            "signals": signals,
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
