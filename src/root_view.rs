use gpui::{
    AnyWindowHandle, App, AsyncApp, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    Render, WeakEntity, Window, div, prelude::*, px,
};
use std::time::Duration;

use crate::device::{self, DeviceState};
use crate::tabs::{ConfigurationTab, DeviceInfoTab, HomeTab, LogsTab, Tab};
use crate::theme::{self, TITLEBAR_HEIGHT, TITLEBAR_LEFT_INSET, TITLEBAR_RIGHT_INSET};
use crate::toast::{self, Toast, ToastKind};

const TOAST_LIFETIME: Duration = Duration::from_secs(4);

pub struct RootView {
    state: DeviceState,
    req_tx: device::RequestTx,
    log_tx: device::LogRequestTx,
    selected_tab: Tab,
    focus_handle: FocusHandle,
    home_tab: HomeTab,
    pub(crate) logs_tab: LogsTab,
    configuration_tab: ConfigurationTab,
    info_tab: DeviceInfoTab,
    logs_poll: Option<gpui::Task<()>>,
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

        Self {
            state: DeviceState::default(),
            req_tx,
            log_tx,
            selected_tab: Tab::Home,
            focus_handle,
            home_tab: HomeTab::default(),
            logs_tab: LogsTab::default(),
            configuration_tab: ConfigurationTab::default(),
            info_tab: DeviceInfoTab::default(),
            logs_poll: None,
            toasts: Vec::new(),
            next_toast_id: 0,
        }
    }

    fn push_toast(&mut self, cx: &mut Context<Self>, message: String, kind: ToastKind) {
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

    fn tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(20.))
            .h(px(TITLEBAR_HEIGHT))
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE));

        for tab in Tab::ALL {
            let is_selected = tab == self.selected_tab;
            let (color, underline) = if is_selected {
                (theme::fg(), theme::accent())
            } else {
                (theme::muted(), gpui::transparent_black())
            };

            row = row.child(
                div()
                    .id(("tab", tab as usize))
                    .h(px(TITLEBAR_HEIGHT))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .border_b_2()
                    .border_color(underline)
                    .text_color(color)
                    .hover(|s| s.text_color(theme::fg()))
                    .cursor_pointer()
                    .child(tab.title())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_tab = tab;
                        this.sync_logs_poll(cx);
                        cx.notify();
                    })),
            );
        }

        row
    }

    fn status_indicator(&self) -> impl IntoElement {
        let connected = self.state.port.is_some();
        let (color, label) = if connected {
            (theme::green(), "connected")
        } else {
            (theme::muted(), "disconnected")
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .child(div().text_color(color).child("●"))
            .child(div().text_color(theme::muted()).child(label))
    }

    fn restart_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let log_tx = self.log_tx.clone();
        theme::button_base("restart-button", "restart")
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .hover(|s| s.text_color(theme::red()).border_color(theme::red()))
            .on_click(cx.listener(move |_, _, _, cx| {
                let log_tx = log_tx.clone();
                cx.spawn(async move |weak, cx| {
                    run_command_toast(weak, cx, log_tx, device::Command::Reboot, "restart").await
                })
                .detach();
            }))
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
                    .top_0()
                    .left(px(TITLEBAR_LEFT_INSET))
                    .child(self.tab_bar(cx)),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(TITLEBAR_RIGHT_INSET))
                    .h(px(TITLEBAR_HEIGHT))
                    .flex()
                    .flex_row()
                    .items_center()
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

impl Focusable for RootView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.selected_tab {
            Tab::Home => self.home_tab.render(&self.state, &self.req_tx),
            Tab::Logs => self
                .logs_tab
                .render(&self.state, &self.req_tx, &self.log_tx, cx),
            Tab::Configuration => self.configuration_tab.render(),
            Tab::Info => self.info_tab.render(&self.state),
        };

        let body = div()
            .max_w(px(640.))
            .size_full()
            .flex()
            .flex_col()
            .pl(px(32.))
            .pr(px(32.))
            .pb(px(32.))
            .pt(px(TITLEBAR_HEIGHT + 24.))
            .child(div().flex_1().min_h(px(0.)).child(content));

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, _| {
                let m = &event.keystroke.modifiers;
                if event.keystroke.key == "q" && !m.control && !m.platform && !m.alt {
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
