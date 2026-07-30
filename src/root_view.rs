use gpui::{
    AnyWindowHandle, App, AsyncApp, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    Render, SharedString, Window, div, prelude::*, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::notification::NotificationType;
use gpui_component::tab::TabBar;
use gpui_component::tag::Tag;
use gpui_component::{Sizable, WindowExt};

use crate::device::{self, DeviceState};
use crate::tabs::{ConfigurationTab, DeviceInfoTab, HomeTab, LogsTab, Tab};
use crate::theme::{self, TITLEBAR_HEIGHT, TITLEBAR_LEFT_INSET, TITLEBAR_RIGHT_INSET};

pub struct RootView {
    state: DeviceState,
    req_tx: device::RequestTx,
    log_tx: device::LogRequestTx,
    selected_tab: Tab,
    focus_handle: FocusHandle,
    home_tab: HomeTab,
    pub(crate) logs_tab: LogsTab,
    pub(crate) configuration_tab: ConfigurationTab,
    info_tab: DeviceInfoTab,
    logs_poll: Option<gpui::Task<()>>,
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
        }
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
                    this.configuration_tab.auto_fetch(&log_tx, window_handle, cx);
                    cx.notify();
                })
                .ok();
            })
    }

    fn status_indicator(&self) -> impl IntoElement {
        let tag = if self.state.port.is_some() {
            Tag::success().outline().child("connected")
        } else {
            Tag::secondary().outline().child("disconnected")
        };
        tag.small().font(theme::mono_font())
    }

    fn restart_button(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let log_tx = self.log_tx.clone();
        Button::new("restart-button")
            .label("restart")
            .danger()
            .ghost()
            .small()
            .on_click(move |_, window, app| {
                let log_tx = log_tx.clone();
                let window_handle = window.window_handle();
                app.spawn(async move |cx| {
                    run_command_notify(window_handle, cx, log_tx, device::Command::Reboot, "restart")
                        .await
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

/// Pushes a notification into the gpui-component `Root` overlay.
pub fn notify(window: AnyWindowHandle, cx: &mut AsyncApp, kind: NotificationType, message: String) {
    cx.update_window(window, |_, window, cx| {
        window.push_notification((kind, SharedString::from(message)), cx);
    })
    .ok();
}

/// Sends `cmd` and reports the result as a notification
pub async fn run_command_notify(
    window: AnyWindowHandle,
    cx: &mut AsyncApp,
    log_tx: device::LogRequestTx,
    cmd: device::Command,
    label: &str,
) {
    let (kind, message) = match device::request(&log_tx, cmd).await {
        Ok(_) => (NotificationType::Success, label.to_string()),
        Err(e) => (NotificationType::Error, format!("{label} failed: {e}")),
    };
    notify(window, cx, kind, message);
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
            Tab::Configuration => self.configuration_tab.render(&self.log_tx, cx),
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
    }
}
