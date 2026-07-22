use gpui::{AnyElement, AsyncApp, Context, IntoElement, WeakEntity, div, prelude::*, px};
use std::path::PathBuf;

use crate::device::{self, DeviceState};
use crate::log_parse;
use crate::root_view::RootView;
use crate::theme;

#[derive(Default)]
pub struct LogsTab {
    entries: Vec<device::LogEntry>,
    status: Status,
    busy: Option<Busy>,
    last_result: Option<Result<String, String>>,
}

#[derive(Default)]
enum Status {
    #[default]
    Idle,
    Loading,
    Error(String),
}

struct Busy {
    name: String,
    downloaded: u64,
    total: u64,
}

impl LogsTab {
    /// Used by the periodic background poll (see `RootView::sync_logs_poll`).
    pub(crate) fn set_entries(&mut self, entries: Vec<device::LogEntry>) {
        self.entries = entries;
    }

    pub fn render(
        &self,
        state: &DeviceState,
        req_tx: &device::RequestTx,
        log_tx: &device::LogRequestTx,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let (status_text, status_color) = match &self.status {
            Status::Idle if self.entries.is_empty() => {
                ("no logs yet — [ refresh ]".to_string(), theme::muted())
            }
            Status::Idle => (format!("{} log(s)", self.entries.len()), theme::muted()),
            Status::Loading => ("loading...".to_string(), theme::amber()),
            Status::Error(e) => (format!("error: {e}"), theme::red()),
        };

        let mut rows = div().flex().flex_col().gap(px(4.));
        for (idx, entry) in self.entries.iter().enumerate() {
            rows = rows.child(self.entry_row(idx, entry, state, log_tx, cx));
        }

        let mut root = div()
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(self.toolbar(state, req_tx, log_tx, status_text, status_color, cx))
            .child(rows);

        if let Some(result) = &self.last_result {
            let (msg, color) = match result {
                Ok(msg) => (msg.clone(), theme::green()),
                Err(msg) => (msg.clone(), theme::red()),
            };
            root = root.child(div().text_color(color).child(msg));
        }

        root.into_any_element()
    }

    fn toolbar(
        &self,
        state: &DeviceState,
        req_tx: &device::RequestTx,
        log_tx: &device::LogRequestTx,
        status_text: String,
        status_color: gpui::Hsla,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let refresh_tx = log_tx.clone();
        let refresh_btn = div()
            .id("logs-refresh")
            .text_color(theme::muted())
            .hover(|s| s.text_color(theme::fg()))
            .cursor_pointer()
            .child("[ refresh ]")
            .on_click(cx.listener(move |this, _, _, cx| {
                this.logs_tab.status = Status::Loading;
                cx.notify();

                let log_tx = refresh_tx.clone();
                cx.spawn(async move |weak, cx| {
                    let result = device::list_logs(&log_tx).await;
                    weak.update(cx, |view, cx| {
                        match result {
                            Ok(entries) => {
                                view.logs_tab.entries = entries;
                                view.logs_tab.status = Status::Idle;
                            }
                            Err(e) => view.logs_tab.status = Status::Error(e.to_string()),
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }));

        let (pause_label, pause_active) = match state.logging_active {
            Some(true) => ("[ pause ]", true),
            Some(false) => ("[ resume ]", true),
            None => ("[ pause ]", false),
        };
        let pause_tx = req_tx.clone();
        let resume = !matches!(state.logging_active, Some(true));
        let pause_btn = div()
            .id("logs-pause")
            .text_color(if pause_active {
                theme::muted()
            } else {
                theme::muted().opacity(0.4)
            })
            .hover(|s| s.text_color(theme::fg()))
            .cursor_pointer()
            .child(pause_label)
            .on_click(move |_, _, _| {
                let _ = pause_tx.send(device::Command::SetLogging { active: resume });
            });

        let next_tx = req_tx.clone();
        let next_btn = div()
            .id("logs-next")
            .text_color(theme::muted())
            .hover(|s| s.text_color(theme::fg()))
            .cursor_pointer()
            .child("[ new log ]")
            .on_click(move |_, _, _| {
                let _ = next_tx.send(device::Command::NextLog);
            });

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(refresh_btn)
            .child(pause_btn)
            .child(next_btn)
            .child(div().text_color(status_color).child(status_text))
            .into_any_element()
    }

    fn entry_row(
        &self,
        idx: usize,
        entry: &device::LogEntry,
        state: &DeviceState,
        log_tx: &device::LogRequestTx,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let is_current = state.current_log.as_deref() == Some(entry.name.as_str())
            && state.logging_active == Some(true);
        let name_color = if is_current {
            theme::green()
        } else {
            theme::fg()
        };

        let busy = self.busy.as_ref().filter(|b| b.name == entry.name);
        let label = if busy.is_some() {
            "[ working... ]"
        } else {
            "[ get ]"
        };

        let log_tx = log_tx.clone();
        let name = entry.name.clone();
        let total = entry.size;
        let get_btn = div()
            .id(("logs-get", idx))
            .text_color(theme::muted())
            .hover(|s| s.text_color(theme::fg()))
            .cursor_pointer()
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.logs_tab.busy.is_some() {
                    return;
                }
                this.logs_tab.busy = Some(Busy {
                    name: name.clone(),
                    downloaded: 0,
                    total,
                });
                this.logs_tab.last_result = None;
                cx.notify();

                let log_tx = log_tx.clone();
                let name = name.clone();
                cx.spawn(async move |weak, cx| run_job(weak, cx, log_tx, name).await)
                    .detach();
            }));

        let row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .text_color(name_color)
                    .w(px(180.))
                    .child(entry.name.clone()),
            )
            .child(
                div()
                    .text_color(theme::muted())
                    .w(px(64.))
                    .child(format_size(entry.size)),
            )
            .child(get_btn);

        match busy {
            Some(b) => div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(row)
                .child(progress_bar(b.downloaded, b.total))
                .into_any_element(),
            None => row.into_any_element(),
        }
    }
}

async fn run_job(weak: WeakEntity<RootView>, cx: &mut AsyncApp, log_tx: device::LogRequestTx, name: String) {
    let progress_weak = weak.clone();
    let mut progress_cx = cx.clone();
    let on_progress = move |downloaded: u64| {
        progress_weak
            .update(&mut progress_cx, |view, cx| {
                if let Some(busy) = &mut view.logs_tab.busy {
                    busy.downloaded = downloaded;
                }
                cx.notify();
            })
            .ok();
    };

    let result = run_job_inner(cx, &log_tx, &name, on_progress).await;
    weak.update(cx, |view, cx| {
        let tab = &mut view.logs_tab;
        tab.busy = None;
        tab.last_result = Some(result.map_err(|e| e.to_string()));
        cx.notify();
    })
    .ok();
}

async fn run_job_inner(
    cx: &mut AsyncApp,
    log_tx: &device::LogRequestTx,
    name: &str,
    on_progress: impl FnMut(u64),
) -> anyhow::Result<String> {
    let csv_name = format!("{}.csv", name.trim_end_matches(".bin"));
    let Some(dest) = prompt_save(cx, &csv_name).await? else {
        return Ok("cancelled".to_string());
    };

    let raw = device::download_log(log_tx, name, on_progress).await?;
    let tmp_path = std::env::temp_dir().join(name);
    std::fs::write(&tmp_path, &raw)?;

    let parsed = log_parse::parse_log(&raw)?;
    std::fs::write(&dest, log_parse::to_csv(&parsed))?;

    Ok(format!(
        "parsed {} row(s), saved to {}",
        parsed.rows.len(),
        dest.display()
    ))
}

async fn prompt_save(cx: &mut AsyncApp, suggested_name: &str) -> anyhow::Result<Option<PathBuf>> {
    let dir = std::env::temp_dir();
    let rx = cx.update(|app| app.prompt_for_new_path(&dir, Some(suggested_name)))?;
    rx.await
        .map_err(|_| anyhow::anyhow!("save dialog closed unexpectedly"))?
}

fn progress_bar(downloaded: u64, total: u64) -> AnyElement {
    const WIDTH: f32 = 120.;
    let fraction = if total == 0 {
        1.0
    } else {
        (downloaded as f32 / total as f32).clamp(0.0, 1.0)
    };

    div()
        .w(px(WIDTH))
        .h(px(5.))
        .bg(theme::muted())
        .child(div().w(px(WIDTH * fraction)).h(px(5.)).bg(theme::green()))
        .into_any_element()
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
