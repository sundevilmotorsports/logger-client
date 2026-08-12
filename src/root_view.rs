use gpui::{
    AnyWindowHandle, App, AsyncApp, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    Render, WeakEntity, Window, div, prelude::*, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::label::Label;
use gpui_component::tab::TabBar;
use gpui_component::{Sizable, h_flex, v_flex};
use std::time::Duration;

use crate::device::{self, DeviceState};
use crate::tabs::{ConfigurationTab, ConsoleTab, DeviceInfoTab, HomeTab, LogsTab, Tab};
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
    pub(crate) console_tab: ConsoleTab,
    pub(crate) configuration_tab: ConfigurationTab,
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

        let mut this = Self {
            state: DeviceState::default(),
            req_tx,
            log_tx,
            selected_tab: Tab::Home,
            focus_handle,
            home_tab: HomeTab::default(),
            logs_tab: LogsTab::default(),
            console_tab: ConsoleTab::default(),
            configuration_tab: ConfigurationTab::default(),
            info_tab: DeviceInfoTab::default(),
            logs_poll: None,
            toasts: Vec::new(),
            next_toast_id: 0,
        };
        
        this.console_tab.start(cx);
        this
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
