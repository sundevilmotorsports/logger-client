use slint::ComponentHandle;
use std::rc::Rc;
use std::time::Duration;

use crate::app::{self, App, model};
use crate::device::{self, CanNode, DeviceState};
use crate::ui::{AppWindow, DeviceRow, DevicesState, Tone};

/// A node is online if its last heartbeat is younger than this. Heartbeats
/// go out about once a second
const ONLINE_MS: i64 = 3_000;

/// Raw bytes per `OtaUpload` serial frame (hex-encoded on the wire, so 2x this).
const UPLOAD_CHUNK: usize = 2048;

#[derive(Default)]
pub struct DevicesTab {
    nodes: Vec<CanNode>,
    /// Node ids in the order they were last rendered, so a row index from
    /// the UI maps back to a node.
    shown: Vec<u8>,
    job: Option<OtaJob>,
    last_result: Option<Result<String, String>>,
}

struct OtaJob {
    node: u8,
    phase: &'static str,
    done: u64,
    total: u64,
}

pub fn wire(app: &Rc<App>, ui: &AppWindow) {
    let app = app.clone();
    ui.global::<DevicesState>().on_update(move |ix| {
        let node = {
            let mut tab = app.devices_tab.borrow_mut();
            let Some(&node) = tab.shown.get(ix as usize) else {
                return;
            };
            if tab.job.is_some() {
                return;
            }
            tab.job = Some(OtaJob {
                node,
                phase: "starting",
                done: 0,
                total: 0,
            });
            tab.last_result = None;
            node
        };
        app.render_devices();

        let app = app.clone();
        app::spawn(async move { run_ota_job(&app, node).await });
    });
}

impl DevicesTab {
    /// Fed by the background poll in `App::sync_devices_poll`.
    pub(crate) fn set_nodes(&mut self, mut nodes: Vec<CanNode>) {
        nodes.sort_by_key(|n| n.node);
        self.nodes = nodes;
    }

    pub fn render(&mut self, state: &DeviceState, ui: &AppWindow) {
        const LOGGER: u8 = sdm_utils::Node::Logger as u8;
        let mut nodes = self.nodes.clone();
        if state.port.is_some() && !nodes.iter().any(|n| n.node == LOGGER) {
            nodes.insert(
                0,
                CanNode {
                    node: LOGGER,
                    device_type: sdm_utils::DeviceType::Logger as u8,
                    age_ms: 0,
                },
            );
        }
        self.shown = nodes.iter().map(|n| n.node).collect();

        let rows: Vec<DeviceRow> = nodes.iter().map(|n| self.node_row(n, state)).collect();

        let s = ui.global::<DevicesState>();
        s.set_rows(model(rows));
        s.set_empty_text(
            if state.port.is_some() {
                "no CAN nodes seen yet"
            } else {
                "no device connected"
            }
            .into(),
        );
        s.set_busy(self.job.is_some());
        let (text, tone) = match &self.last_result {
            Some(Ok(m)) => (m.as_str(), Tone::Green),
            Some(Err(m)) => (m.as_str(), Tone::Red),
            None => ("", Tone::Muted),
        };
        s.set_result_text(text.into());
        s.set_result_tone(tone);
    }

    fn node_row(&self, node: &CanNode, state: &DeviceState) -> DeviceRow {
        let is_self = node.node == sdm_utils::Node::Logger as u8;
        let dtype = sdm_utils::DeviceType::from_byte(node.device_type);
        let type_label = match dtype {
            sdm_utils::DeviceType::Unknown => {
                format!("{} (0x{:02X})", dtype.name(), node.device_type)
            }
            _ => dtype.name().to_string(),
        };

        let (status, status_tone) = if is_self {
            let label = match &state.firmware_version {
                Some(v) => v.clone(),
                None => "this device".to_string(),
            };
            (label, Tone::Green)
        } else if node.age_ms < ONLINE_MS {
            ("online".to_string(), Tone::Green)
        } else {
            (format!("{}s ago", node.age_ms / 1000), Tone::Muted)
        };

        let job = self.job.as_ref().filter(|j| j.node == node.node);
        let action = if job.is_some() {
            "updating..."
        } else if is_self {
            "update"
        } else {
            "update fw"
        };

        DeviceRow {
            node: format!("0x{:02X}", node.node).into(),
            r#type: type_label.into(),
            status: status.into(),
            status_tone,
            action: action.into(),
            busy: job.is_some(),
            phase: job.map(|j| j.phase).unwrap_or_default().into(),
            progress: job
                .filter(|j| j.total > 0)
                .map(|j| j.done as f32 / j.total as f32)
                .unwrap_or(0.),
        }
    }
}

async fn run_ota_job(app: &Rc<App>, node: u8) {
    let result = run_ota_job_inner(app, node).await;
    {
        let mut tab = app.devices_tab.borrow_mut();
        tab.job = None;
        tab.last_result = Some(result.map_err(|e| e.to_string()));
    }
    app.render_devices();
}

async fn run_ota_job_inner(app: &Rc<App>, node: u8) -> anyhow::Result<String> {
    let Some(path) = app::pick_file("Select firmware .bin").await else {
        return Ok("cancelled".to_string());
    };
    let bytes = std::fs::read(&path)?;
    if bytes.is_empty() {
        anyhow::bail!("firmware file is empty");
    }
    let crc = sdm_utils::ota::crc32(&bytes);
    let total = bytes.len() as u64;
    let is_logger = node == sdm_utils::Node::Logger as u8;
    let phase = if is_logger {
        "writing to logger flash"
    } else {
        "streaming to node"
    };
    let log_tx = &app.log_tx;

    match device::request(
        log_tx,
        device::Command::OtaFlash {
            node,
            size: bytes.len() as u32,
            crc,
        },
    )
    .await?
    {
        device::Response::OtaFlashOk => {}
        other => anyhow::bail!("unexpected response to ota_flash: {other:?}"),
    }

    set_job(app, phase, 0, total);
    for (i, chunk) in bytes.chunks(UPLOAD_CHUNK).enumerate() {
        let offset = (i * UPLOAD_CHUNK) as u64;
        let committed = match device::request(
            log_tx,
            device::Command::OtaUpload {
                offset,
                data: device::hex_encode(chunk),
            },
        )
        .await?
        {
            device::Response::OtaUpload { committed } => committed,
            other => anyhow::bail!("unexpected response to ota_upload: {other:?}"),
        };
        set_job(app, phase, committed as u64, total);
    }

    set_job(app, "verifying", 0, total);
    let mut last_sent = 0u32;
    let mut stalled_polls = 0u32;
    loop {
        app::sleep(Duration::from_millis(500)).await;
        let status = match device::request(log_tx, device::Command::OtaStatus).await {
            Ok(device::Response::OtaStatus(s)) => s,
            Ok(other) => anyhow::bail!("unexpected response to ota_status: {other:?}"),
            // A successful logger self-update reboots the logger mid-poll.
            Err(_) if is_logger => return Ok("logger updated, rebooting".to_string()),
            Err(e) => return Err(e),
        };
        set_job(
            app,
            "verifying",
            status.sent as u64,
            status.total.max(1) as u64,
        );

        stalled_polls = if status.sent == last_sent {
            stalled_polls + 1
        } else {
            0
        };
        last_sent = status.sent;
        if status.result.is_none() && stalled_polls >= 120 {
            anyhow::bail!("no progress from the logger — is the CAN bus active?");
        }
        if let Some(code) = status.result {
            return match code {
                0 if is_logger => Ok("logger updated, rebooting".to_string()),
                0 => Ok(format!("node 0x{node:02X} updated")),
                4 => anyhow::bail!("transfer corrupted (CRC mismatch)"),
                5 if is_logger => anyhow::bail!("image rejected by esp_ota"),
                5 => anyhow::bail!("node 0x{node:02X} flash write failed"),
                0xFE => anyhow::bail!("node 0x{node:02X} never acknowledged"),
                0xF5 => anyhow::bail!("wrong file"),
                0xFD | 0xF4 => {
                    anyhow::bail!("upload stalled before the image was complete")
                }
                n => anyhow::bail!("update rejected (can_ota code {n})"),
            };
        }
    }
}

fn set_job(app: &App, phase: &'static str, done: u64, total: u64) {
    if let Some(job) = &mut app.devices_tab.borrow_mut().job {
        job.phase = phase;
        job.done = done;
        job.total = total;
    }
    app.render_devices();
}
