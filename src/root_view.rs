use gpui::{
    AnyWindowHandle, App, AsyncApp, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    PathPromptOptions, Render, WeakEntity, Window, actions, div, prelude::*, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::label::Label;
use gpui_component::tab::TabBar;
use gpui_component::{Sizable, h_flex, v_flex};
use std::path::PathBuf;
use std::time::Duration;

use crate::device::{self, DeviceState};
use crate::log_parse;
use crate::tabs::{ConfigurationTab, ConsoleTab, DeviceInfoTab, DevicesTab, HomeTab, LogsTab, Tab};
use crate::theme::{self, TITLEBAR_HEIGHT, TITLEBAR_LEFT_INSET, TITLEBAR_RIGHT_INSET};
use crate::toast::{self, Toast, ToastKind};

const TOAST_LIFETIME: Duration = Duration::from_secs(4);

actions!(logger_client, [ParseLogFile, ParseLogFolder]);

pub struct RootView {
    state: DeviceState,
    req_tx: device::RequestTx,
    log_tx: device::LogRequestTx,
    selected_tab: Tab,
    focus_handle: FocusHandle,
    home_tab: HomeTab,
    pub(crate) devices_tab: DevicesTab,
    pub(crate) logs_tab: LogsTab,
    pub(crate) console_tab: ConsoleTab,
    pub(crate) configuration_tab: ConfigurationTab,
    info_tab: DeviceInfoTab,
    logs_poll: Option<gpui::Task<()>>,
    devices_poll: Option<gpui::Task<()>>,
    toasts: Vec<(u64, Toast)>,
    next_toast_id: u64,
}

impl RootView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (tx, mut rx) = tokio::sync::watch::channel(DeviceState::default());
        let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel();
        let (log_tx, log_rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || device::poll(tx, req_rx, log_rx));

        let window_handle: AnyWindowHandle = window.window_handle();
        cx.spawn(async move |this, cx| {
            while rx.changed().await.is_ok() {
                let state = rx.borrow().clone();
                log::debug!("root_view: received device state, port={:?}", state.port);
                let title = state
                    .port
                    .as_deref()
                    .map(|p| format!("● {p}"))
                    .unwrap_or_else(|| "Logger Client".to_string());

                if this
                    .update(cx, |view, cx| {
                        view.state = state;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                cx.update_window(window_handle, |_, window, _| {
                    window.set_window_title(&title);
                })
                .ok();
            }
        })
        .detach();

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);

        let mut this = Self {
            state: DeviceState::default(),
            req_tx,
            log_tx,
            selected_tab: Tab::Home,
            focus_handle,
            home_tab: HomeTab::default(),
            devices_tab: DevicesTab::default(),
            logs_tab: LogsTab::default(),
            console_tab: ConsoleTab::default(),
            configuration_tab: ConfigurationTab::default(),
            info_tab: DeviceInfoTab::default(),
            logs_poll: None,
            devices_poll: None,
            toasts: Vec::new(),
            next_toast_id: 0,
        };

        this.console_tab.start(cx);
        this
    }

    /// `File > Parse Log File...`: pick a `.bin` log already sitting on disk
    /// (e.g. pulled straight off the SD card) and decode it to CSV, with no
    /// device connection required.
    fn on_parse_log_file(
        &mut self,
        _: &ParseLogFile,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| parse_log_file_flow(weak, cx).await)
            .detach();
    }

    /// `File > Parse Log Folder...`: pick a directory and decode every
    /// `.bin` log inside it to CSV + `.ld`, with no device connection
    /// required.
    fn on_parse_log_folder(
        &mut self,
        _: &ParseLogFolder,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| parse_log_folder_flow(weak, cx).await)
            .detach();
    }

    pub(crate) fn push_toast(&mut self, cx: &mut Context<Self>, message: String, kind: ToastKind) {
        let id = self.next_toast_id;
        self.next_toast_id += 1;
        self.toasts.push((id, Toast { message, kind }));
        cx.notify();

        cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(TOAST_LIFETIME).await;
            weak.update(cx, |view, cx| {
                view.toasts.retain(|(tid, _)| *tid != id);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Keeps `logs_tab.entries` fresh once a second while the Logs tab is open;
    /// dropping the task (by setting it to `None`) cancels the loop.
    fn sync_logs_poll(&mut self, cx: &mut Context<Self>) {
        if self.selected_tab != Tab::Logs {
            self.logs_poll = None;
            return;
        }
        if self.logs_poll.is_some() {
            return;
        }
        let log_tx = self.log_tx.clone();
        self.logs_poll = Some(cx.spawn(async move |weak, cx| {
            loop {
                let result = device::list_logs(&log_tx).await;
                let alive = weak
                    .update(cx, |view, cx| {
                        if let Ok(entries) = result {
                            view.logs_tab.set_entries(entries);
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    return;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
            }
        }));
    }

    /// Same idea as `sync_logs_poll`, for the Devices tab's CAN node list.
    fn sync_devices_poll(&mut self, cx: &mut Context<Self>) {
        if self.selected_tab != Tab::Devices {
            self.devices_poll = None;
            return;
        }
        if self.devices_poll.is_some() {
            return;
        }
        let log_tx = self.log_tx.clone();
        self.devices_poll = Some(cx.spawn(async move |weak, cx| {
            loop {
                let result = device::can_nodes(&log_tx).await;
                let alive = weak
                    .update(cx, |view, cx| {
                        if let Ok(nodes) = result {
                            view.devices_tab.set_nodes(nodes);
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    return;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
            }
        }));
    }

    fn tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        TabBar::new("tabs")
            .underline()
            .small()
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .selected_index(self.selected_tab as usize)
            .children(Tab::ALL.map(|t| t.title()))
            .on_click(move |ix, window, app| {
                let tab = Tab::ALL[*ix];
                let window_handle = window.window_handle();
                weak.update(app, |this, cx| {
                    this.selected_tab = tab;
                    this.sync_logs_poll(cx);
                    this.sync_devices_poll(cx);
                    let log_tx = this.log_tx.clone();
                    this.configuration_tab
                        .auto_fetch(&log_tx, window_handle, cx);
                    cx.notify();
                })
                .ok();
            })
    }

    fn status_indicator(&self) -> impl IntoElement {
        let (color, label) = if self.state.port.is_some() {
            (theme::green(), "connected")
        } else {
            (theme::muted(), "disconnected")
        };

        h_flex()
            .gap(px(6.))
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .child(div().text_color(color).child("●"))
            .child(Label::new(label).text_color(theme::muted()))
    }

    fn restart_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let log_tx = self.log_tx.clone();
        let weak = cx.weak_entity();
        Button::new("restart-button")
            .label("restart")
            .danger()
            .ghost()
            .small()
            .on_click(move |_, _, app| {
                let log_tx = log_tx.clone();
                let weak = weak.clone();
                app.spawn(async move |cx| {
                    run_command_toast(weak, cx, log_tx, device::Command::Reboot, "restart").await
                })
                .detach();
            })
    }

    /// A full-width strip behind the tabs, colored differently from the body.
    fn title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .h(px(TITLEBAR_HEIGHT))
            .bg(theme::titlebar_bg())
            .border_b_1()
            .border_color(theme::border())
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left(px(TITLEBAR_LEFT_INSET))
                    .child(self.tab_bar(cx)),
            )
            .child(
                h_flex()
                    .absolute()
                    .top_0()
                    .right(px(TITLEBAR_RIGHT_INSET))
                    .h(px(TITLEBAR_HEIGHT))
                    .gap(px(16.))
                    .child(self.status_indicator())
                    .child(self.restart_button(cx)),
            )
    }
}

/// Sends `cmd` and reports the result as a toast
pub async fn run_command_toast(
    weak: WeakEntity<RootView>,
    cx: &mut AsyncApp,
    log_tx: device::LogRequestTx,
    cmd: device::Command,
    label: &str,
) {
    let result = device::request(&log_tx, cmd).await;
    let (message, kind) = match result {
        Ok(_) => (label.to_string(), ToastKind::Success),
        Err(e) => (format!("{label} failed: {e}"), ToastKind::Error),
    };
    weak.update(cx, |view, cx| view.push_toast(cx, message, kind))
        .ok();
}

/// Prompts for a local `.bin` log file, decodes it, prompts for where to
/// save the CSV, and reports the result as a toast. Independent of any live
/// device connection -- for logs already off the SD card.
async fn parse_log_file_flow(weak: WeakEntity<RootView>, cx: &mut AsyncApp) {
    let result = parse_log_file_flow_inner(cx).await;
    let (message, kind) = match result {
        Ok(Some(message)) => (message, ToastKind::Success),
        Ok(None) => return, // user cancelled a dialog; nothing to report
        Err(e) => (format!("parse log failed: {e}"), ToastKind::Error),
    };
    weak.update(cx, |view, cx| view.push_toast(cx, message, kind))
        .ok();
}

async fn parse_log_file_flow_inner(cx: &mut AsyncApp) -> anyhow::Result<Option<String>> {
    let open_rx = cx.update(|app| {
        app.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Parse".into()),
        })
    })?;
    let Some(mut paths) = open_rx
        .await
        .map_err(|_| anyhow::anyhow!("open dialog closed unexpectedly"))??
    else {
        return Ok(None);
    };
    let src = paths.remove(0);

    let raw =
        std::fs::read(&src).map_err(|e| anyhow::anyhow!("couldn't read {}: {e}", src.display()))?;

    let csv_name = format!(
        "{}.csv",
        src.file_stem()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default()
    );
    let save_dir = src
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let save_rx = cx.update(|app| app.prompt_for_new_path(&save_dir, Some(&csv_name)))?;
    let Some(dest) = save_rx
        .await
        .map_err(|_| anyhow::anyhow!("save dialog closed unexpectedly"))??
    else {
        return Ok(None);
    };

    // Parsing is synchronous CPU work; run it off the UI thread like the
    // device-download path does.
    let parse_handle = std::thread::spawn(move || log_parse::parse_log(&raw, |_, _| {}));
    let parsed = parse_handle
        .join()
        .map_err(|_| anyhow::anyhow!("log parser thread panicked"))??;

    std::fs::write(&dest, log_parse::to_csv(&parsed))
        .map_err(|e| anyhow::anyhow!("couldn't write {}: {e}", dest.display()))?;

    let ld_dest = dest.with_extension("ld");
    std::fs::write(&ld_dest, crate::motec::build_ld(&parsed))
        .map_err(|e| anyhow::anyhow!("couldn't write {}: {e}", ld_dest.display()))?;

    Ok(Some(format!(
        "parsed {} rows from {}, saved {} and {}",
        parsed.rows.len(),
        src.display(),
        dest.display(),
        ld_dest.display()
    )))
}

/// Prompts for a directory, decodes every `.bin` log file inside it (next to
/// itself as `<name>.csv` and `<name>.ld`), and reports a summary as a
/// toast. Independent of any live device connection.
async fn parse_log_folder_flow(weak: WeakEntity<RootView>, cx: &mut AsyncApp) {
    let result = parse_log_folder_flow_inner(cx).await;
    let (message, kind) = match result {
        Ok(Some(message)) => (message, ToastKind::Success),
        Ok(None) => return, // user cancelled the dialog; nothing to report
        Err(e) => (format!("parse log folder failed: {e}"), ToastKind::Error),
    };
    weak.update(cx, |view, cx| view.push_toast(cx, message, kind))
        .ok();
}

async fn parse_log_folder_flow_inner(cx: &mut AsyncApp) -> anyhow::Result<Option<String>> {
    let open_rx = cx.update(|app| {
        app.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Parse Folder".into()),
        })
    })?;
    let Some(mut paths) = open_rx
        .await
        .map_err(|_| anyhow::anyhow!("open dialog closed unexpectedly"))??
    else {
        return Ok(None);
    };
    let dir = paths.remove(0);

    let mut bin_paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| anyhow::anyhow!("couldn't read {}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|ext| ext == "bin"))
        .collect();
    bin_paths.sort();

    if bin_paths.is_empty() {
        return Ok(Some(format!("no .bin log files found in {}", dir.display())));
    }

    // Parsing is synchronous CPU work; run the whole batch off the UI thread
    // like the single-file path does.
    let parse_handle = std::thread::spawn(move || {
        let mut ok_count = 0usize;
        let mut errors: Vec<String> = Vec::new();
        for src in &bin_paths {
            match parse_one_log_to_csv_and_ld(src) {
                Ok(()) => ok_count += 1,
                Err(e) => errors.push(format!(
                    "{}: {e}",
                    src.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()
                )),
            }
        }
        (ok_count, bin_paths.len(), errors)
    });
    let (ok_count, total, errors) = parse_handle
        .join()
        .map_err(|_| anyhow::anyhow!("log parser thread panicked"))?;

    let summary = if errors.is_empty() {
        format!(
            "parsed {ok_count} log file(s) in {}",
            dir.display()
        )
    } else {
        format!(
            "parsed {ok_count}/{total} log file(s) in {} ({} failed: {})",
            dir.display(),
            errors.len(),
            errors.join("; ")
        )
    };

    Ok(Some(summary))
}

/// Decodes a single `.bin` log at `src` and writes `<stem>.csv` and
/// `<stem>.ld` alongside it. Runs the (synchronous, CPU-bound) parser on the
/// calling thread -- callers batching many files should already be off the
/// UI thread.
fn parse_one_log_to_csv_and_ld(src: &PathBuf) -> anyhow::Result<()> {
    let raw =
        std::fs::read(src).map_err(|e| anyhow::anyhow!("couldn't read {}: {e}", src.display()))?;
    let parsed = log_parse::parse_log(&raw, |_, _| {})?;

    let csv_dest = src.with_extension("csv");
    std::fs::write(&csv_dest, log_parse::to_csv(&parsed))
        .map_err(|e| anyhow::anyhow!("couldn't write {}: {e}", csv_dest.display()))?;

    let ld_dest = src.with_extension("ld");
    std::fs::write(&ld_dest, crate::motec::build_ld(&parsed))
        .map_err(|e| anyhow::anyhow!("couldn't write {}: {e}", ld_dest.display()))?;

    Ok(())
}

impl Focusable for RootView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.selected_tab {
            Tab::Home => self.home_tab.render(&self.state, &self.req_tx),
            Tab::Devices => self.devices_tab.render(&self.state, &self.log_tx, cx),
            Tab::Logs => self
                .logs_tab
                .render(&self.state, &self.req_tx, &self.log_tx, cx),
            Tab::Console => self.console_tab.render(cx),
            Tab::Configuration => self.configuration_tab.render(&self.log_tx, cx),
            Tab::Info => self.info_tab.render(&self.state),
        };

        // Console wants the full window width for its log lines, and
        // Configuration needs it for the CAN reference panel; every other
        // tab stays capped to a comfortable reading column.
        let body = v_flex()
            .when(
                !matches!(self.selected_tab, Tab::Console | Tab::Configuration),
                |el| el.max_w(px(640.)),
            )
            .size_full()
            .pl(px(32.))
            .pr(px(32.))
            .pb(px(32.))
            .pt(px(TITLEBAR_HEIGHT + 24.))
            .child(div().flex_1().min_h(px(0.)).child(content));

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_parse_log_file))
            .on_action(cx.listener(Self::on_parse_log_folder))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // Key events bubble from whatever's focused up through this
                // root handler, so without this check typing "q" into a
                // Configuration tab text field would quit the app.
                let root_focused = window.focused(cx).is_none_or(|f| f == this.focus_handle);
                let m = &event.keystroke.modifiers;
                if root_focused && event.keystroke.key == "q" && !m.control && !m.platform && !m.alt
                {
                    std::process::exit(0);
                }
            }))
            .relative()
            .size_full()
            .bg(theme::bg())
            .child(body)
            .child(self.title_bar(cx))
            .child(toast::render(&self.toasts))
    }
}
