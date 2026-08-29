use gpui::{AnyElement, AsyncApp, Context, PathPromptOptions, WeakEntity, div, prelude::*, px};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::label::Label;
use gpui_component::list::ListItem;
use gpui_component::progress::Progress;
use gpui_component::{Disableable, Sizable, h_flex, v_flex};
use std::path::PathBuf;
use std::time::Duration;

use crate::device::{self, CanNode};
use crate::root_view::RootView;
use crate::theme;

/// A node is online if its last heartbeat is younger than this. Heartbeats
/// go out about once a second
const ONLINE_MS: i64 = 3_000;

/// Raw bytes per `OtaUpload` serial frame (hex-encoded on the wire, so 2x this).
const UPLOAD_CHUNK: usize = 2048;

#[derive(Default)]
pub struct DevicesTab {
    nodes: Vec<CanNode>,
    job: Option<OtaJob>,
    last_result: Option<Result<String, String>>,
}

struct OtaJob {
    node: u8,
    phase: &'static str,
    done: u64,
    total: u64,
}

impl DevicesTab {
    /// Fed by the background poll in `RootView::sync_devices_poll`.
    pub(crate) fn set_nodes(&mut self, mut nodes: Vec<CanNode>) {
        nodes.sort_by_key(|n| n.node);
        self.nodes = nodes;
    }

    pub fn render(
        &self,
        log_tx: &device::LogRequestTx,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let body: AnyElement = if self.nodes.is_empty() {
            super::empty_state("no CAN nodes seen yet")
        } else {
            let mut rows = v_flex().items_start().gap(px(2.));
            for (idx, node) in self.nodes.iter().enumerate() {
                rows = rows.child(self.node_row(idx, node, log_tx, cx));
            }
            rows.into_any_element()
        };

        let mut root = v_flex()
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .size_full()
            .gap(px(12.))
            .child(
                h_flex()
                    .gap(px(12.))
                    .child(Label::new("node").text_color(theme::muted()).w(px(52.)))
                    .child(Label::new("type").text_color(theme::muted()).w(px(110.)))
                    .child(Label::new("status").text_color(theme::muted())),
            )
            .child(
                div()
                    .id("devices-scroll")
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h(px(0.))
                    .child(body),
            );

        if let Some(result) = &self.last_result {
            let (msg, color) = match result {
                Ok(m) => (m.clone(), theme::green()),
                Err(m) => (m.clone(), theme::red()),
            };
            root = root.child(Label::new(msg).text_color(color));
        }

        root.into_any_element()
    }

    fn node_row(
        &self,
        idx: usize,
        node: &CanNode,
        log_tx: &device::LogRequestTx,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let is_self = node.node == sdm_utils::Node::Logger as u8;
        let dtype = sdm_utils::DeviceType::from_byte(node.device_type);
        let type_label = match dtype {
            sdm_utils::DeviceType::Unknown => {
                format!("{} (0x{:02X})", dtype.name(), node.device_type)
            }
            _ => dtype.name().to_string(),
        };

        let (status, status_color) = if is_self {
            ("this device".to_string(), theme::green())
        } else if node.age_ms < ONLINE_MS {
            ("online".to_string(), theme::green())
        } else {
            (format!("{}s ago", node.age_ms / 1000), theme::muted())
        };

        let job = self.job.as_ref().filter(|j| j.node == node.node);
        let busy_anywhere = self.job.is_some();

        let node_id = node.node;
        let log_tx = log_tx.clone();
        let weak = cx.weak_entity();
        let update_btn = Button::new(("devices-ota", idx))
            .label(if job.is_some() {
                "updating..."
            } else if is_self {
                "update this logger"
            } else {
                "update fw"
            })
            .ghost()
            .small()
            .disabled(busy_anywhere)
            .on_click(move |_, _, app| {
                let log_tx = log_tx.clone();
                let weak = weak.clone();
                weak.update(app, |this, cx| {
                    if this.devices_tab.job.is_some() {
                        return;
                    }
                    this.devices_tab.job = Some(OtaJob {
                        node: node_id,
                        phase: "starting",
                        done: 0,
                        total: 0,
                    });
                    this.devices_tab.last_result = None;
                    cx.notify();
                    cx.spawn(async move |weak, cx| run_ota_job(weak, cx, log_tx, node_id).await)
                        .detach();
                })
                .ok();
            });

        let row = ListItem::new(("devices-row", idx))
            .text_size(px(theme::FONT_SIZE))
            .rounded_sm()
            .child(
                h_flex()
                    .gap(px(12.))
                    .child(
                        Label::new(format!("0x{:02X}", node.node))
                            .text_color(theme::fg())
                            .w(px(52.)),
                    )
                    .child(Label::new(type_label).text_color(theme::fg()).w(px(110.)))
                    .child(Label::new(status).text_color(status_color).flex_1())
                    .child(update_btn),
            );

        match job {
            Some(j) => v_flex()
                .gap(px(2.))
                .child(row)
                .child(
                    h_flex()
                        .px(px(12.))
                        .gap(px(8.))
                        .child(Label::new(j.phase).text_color(theme::muted()))
                        .child(progress_bar(j.done, j.total)),
                )
                .into_any_element(),
            None => row.into_any_element(),
        }
    }
}

async fn run_ota_job(
    weak: WeakEntity<RootView>,
    cx: &mut AsyncApp,
    log_tx: device::LogRequestTx,
    node: u8,
) {
    let result = run_ota_job_inner(&weak, cx, &log_tx, node).await;
    weak.update(cx, |this, cx| {
        this.devices_tab.job = None;
        this.devices_tab.last_result = Some(result.map_err(|e| e.to_string()));
        cx.notify();
    })
    .ok();
}

async fn run_ota_job_inner(
    weak: &WeakEntity<RootView>,
    cx: &mut AsyncApp,
    log_tx: &device::LogRequestTx,
    node: u8,
) -> anyhow::Result<String> {
    let Some(path) = prompt_open(cx).await? else {
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

    set_job(weak, cx, phase, 0, total);
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
        set_job(weak, cx, phase, committed as u64, total);
    }

    set_job(weak, cx, "verifying", 0, total);
    let mut last_sent = 0u32;
    let mut stalled_polls = 0u32;
    loop {
        cx.background_executor()
            .timer(Duration::from_millis(500))
            .await;
        let status = match device::request(log_tx, device::Command::OtaStatus).await {
            Ok(device::Response::OtaStatus(s)) => s,
            Ok(other) => anyhow::bail!("unexpected response to ota_status: {other:?}"),
            // A successful logger self-update reboots the logger mid-poll.
            Err(_) if is_logger => return Ok("logger updated, rebooting".to_string()),
            Err(e) => return Err(e),
        };
        set_job(
            weak,
            cx,
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
                5 => anyhow::bail!("logger flash write failed"),
                0xFE => anyhow::bail!("node 0x{node:02X} never acknowledged"),
                0xFD | 0xF4 => {
                    anyhow::bail!("upload stalled before the image was complete")
                }
                n => anyhow::bail!("update rejected (can_ota code {n})"),
            };
        }
    }
}

fn set_job(weak: &WeakEntity<RootView>, cx: &mut AsyncApp, phase: &'static str, done: u64, total: u64) {
    weak.update(cx, |this, cx| {
        if let Some(job) = &mut this.devices_tab.job {
            job.phase = phase;
            job.done = done;
            job.total = total;
            cx.notify();
        }
    })
    .ok();
}

async fn prompt_open(cx: &mut AsyncApp) -> anyhow::Result<Option<PathBuf>> {
    let rx = cx.update(|app| {
        app.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select firmware .bin".into()),
        })
    })?;
    let paths = rx
        .await
        .map_err(|_| anyhow::anyhow!("file dialog closed unexpectedly"))??;
    Ok(paths.and_then(|p| p.into_iter().next()))
}

fn progress_bar(done: u64, total: u64) -> AnyElement {
    let percent = if total == 0 {
        0.
    } else {
        done as f32 / total as f32 * 100.
    };
    Progress::new().value(percent).w_full().into_any_element()
}
