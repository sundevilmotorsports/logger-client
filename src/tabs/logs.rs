use gpui::{AnyElement, AsyncApp, Context, IntoElement, WeakEntity, div, prelude::*, px};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::label::Label;
use gpui_component::list::ListItem;
use gpui_component::progress::Progress;
use gpui_component::{Disableable, Sizable, h_flex, v_flex};
use std::path::PathBuf;
use std::time::Duration;

use crate::device::{self, DeviceState};
use crate::log_parse;
use crate::root_view::{self, RootView};
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
    /// Sorted newest first -- names are zero-padded ("0001.bin", "0002.bin",
    /// ...) so a plain string sort already puts them in numeric order.
    pub(crate) fn set_entries(&mut self, mut entries: Vec<device::LogEntry>) {
        entries.sort_by(|a, b| b.name.cmp(&a.name));
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
            Status::Idle if self.entries.is_empty() => (String::new(), theme::muted()),
            Status::Idle => (format!("{} log(s)", self.entries.len()), theme::muted()),
            Status::Loading => ("loading...".to_string(), theme::amber()),
            Status::Error(e) => (format!("error: {e}"), theme::red()),
        };

        let body: AnyElement = if self.entries.is_empty() {
            super::empty_state("no logs yet")
        } else {
            let mut rows = v_flex().items_start().gap(px(2.));
            for (idx, entry) in self.entries.iter().enumerate() {
                rows = rows.child(self.entry_row(idx, entry, state, log_tx, cx));
            }
            rows.into_any_element()
        };

        let mut root = v_flex()
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .size_full()
            .gap(px(12.))
            .child(self.toolbar(state, req_tx, log_tx, status_text, status_color, cx))
            .child(
                div()
                    .id("logs-scroll")
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h(px(0.))
                    .child(body),
            );

        if let Some(result) = &self.last_result {
            let (msg, color) = match result {
                Ok(msg) => (msg.clone(), theme::green()),
                Err(msg) => (msg.clone(), theme::red()),
            };
            root = root.child(Label::new(msg).text_color(color));
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
        let (pause_label, pause_active) = match state.logging_active {
            Some(true) => ("pause", true),
            Some(false) => ("resume", true),
            None => ("pause", false),
        };
        let resume = !matches!(state.logging_active, Some(true));
        let pause_log_tx = log_tx.clone();
        let weak = cx.weak_entity();
        let pause_btn = Button::new("logs-pause")
            .label(pause_label)
            .small()
            .disabled(!pause_active)
            .on_click(move |_, _, app| {
                let log_tx = pause_log_tx.clone();
                let weak = weak.clone();
                app.spawn(async move |cx| {
                    let label = if resume { "resume" } else { "pause" };
                    root_view::run_command_toast(
                        weak,
                        cx,
                        log_tx,
                        device::Command::SetLogging { active: resume },
                        label,
                    )
                    .await
                })
                .detach();
            });

        let next_log_tx = log_tx.clone();
        let weak = cx.weak_entity();
        let next_btn = Button::new("logs-next")
            .label("new log")
            .small()
            .on_click(move |_, _, app| {
                let log_tx = next_log_tx.clone();
                let weak = weak.clone();
                app.spawn(async move |cx| {
                    root_view::run_command_toast(
                        weak,
                        cx,
                        log_tx,
                        device::Command::NextLog,
                        "new log",
                    )
                    .await
                })
                .detach();
            });

        h_flex()
            .justify_between()
            .gap(px(8.))
            .child(h_flex().gap(px(8.)).child(pause_btn).child(next_btn))
            .child(Label::new(status_text).text_color(status_color))
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
        let label = if busy.is_some() { "working..." } else { "get" };

        let log_tx = log_tx.clone();
        let name = entry.name.clone();
        let total = entry.size;
        let weak = cx.weak_entity();
        let get_btn = Button::new(("logs-get", idx))
            .label(label)
            .ghost()
            .small()
            .disabled(busy.is_some())
            .on_click(move |_, _, app| {
                let log_tx = log_tx.clone();
                let name = name.clone();
                let weak = weak.clone();
                weak.update(app, |this, cx| {
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

                    cx.spawn(async move |weak, cx| run_job(weak, cx, log_tx, name).await)
                        .detach();
                })
                .ok();
            });

        let row = ListItem::new(("logs-row", idx))
            .text_size(px(theme::FONT_SIZE))
            .rounded_sm()
            .child(
                h_flex()
                    .gap(px(12.))
                    .child(Label::new(entry.name.clone()).text_color(name_color).w(px(180.)))
                    .child(
                        Label::new(format_size(entry.size))
                            .text_color(theme::muted())
                            .w(px(64.)),
                    )
                    .child(get_btn),
            );

        match busy {
            Some(b) => v_flex()
                .gap(px(2.))
                .child(row)
                .child(
                    div()
                        .px(px(12.))
                        .child(progress_bar(b.downloaded, b.total)),
                )
                .into_any_element(),
            None => row.into_any_element(),
        }
    }
}

fn progress_callback(weak: WeakEntity<RootView>, mut cx: AsyncApp) -> impl FnMut(u64) {
    move |downloaded: u64| {
        weak.update(&mut cx, |view, cx| {
            if let Some(busy) = &mut view.logs_tab.busy {
                busy.downloaded = downloaded;
            }
            cx.notify();
        })
        .ok();
    }
}

async fn run_job(
    weak: WeakEntity<RootView>,
    cx: &mut AsyncApp,
    log_tx: device::LogRequestTx,
    name: String,
) {
    let on_download_progress = progress_callback(weak.clone(), cx.clone());
    let on_parse_progress = progress_callback(weak.clone(), cx.clone());

    let result = run_job_inner(cx, &log_tx, &name, on_download_progress, on_parse_progress).await;
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
    on_download_progress: impl FnMut(u64),
    mut on_parse_progress: impl FnMut(u64),
) -> anyhow::Result<String> {
    let csv_name = format!("{}.csv", name.trim_end_matches(".bin"));
    let Some(dest) = prompt_save(cx, &csv_name).await? else {
        return Ok("cancelled".to_string());
    };

    let raw = device::download_log(log_tx, name, on_download_progress).await?;
    let tmp_path = std::env::temp_dir().join(name);
    std::fs::write(&tmp_path, &raw)?;

    on_parse_progress(0);
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<u64>();
    let parse_handle = std::thread::spawn(move || {
        log_parse::parse_log(&raw, move |done, _total| {
            let _ = progress_tx.send(done as u64);
        })
    });
    while let Some(done) = progress_rx.recv().await {
        on_parse_progress(done);
        cx.background_executor()
            .timer(Duration::from_millis(1))
            .await;
    }
    let parsed = parse_handle
        .join()
        .map_err(|_| anyhow::anyhow!("log parser thread panicked"))??;

    std::fs::write(&dest, log_parse::to_csv(&parsed))?;

    Ok(format!(
        "parsed {} rows, saved to {}",
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
    let percent = if total == 0 {
        100.
    } else {
        downloaded as f32 / total as f32 * 100.
    };
    Progress::new().value(percent).w_full().into_any_element()
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
