use gpui::{
    AnyWindowHandle, App, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, Render,
    Window, div, prelude::*, px,
};

use crate::device::{self, DeviceState};
use crate::tabs::{ConfigurationTab, HomeTab, LogsTab, Tab};
use crate::theme::{self, TITLEBAR_HEIGHT, TITLEBAR_LEFT_INSET};

pub struct RootView {
    state: DeviceState,
    req_tx: device::RequestTx,
    selected_tab: Tab,
    focus_handle: FocusHandle,
    home_tab: HomeTab,
    logs_tab: LogsTab,
    configuration_tab: ConfigurationTab,
}

impl RootView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (tx, mut rx) = tokio::sync::watch::channel(DeviceState::default());
        let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || device::poll(tx, req_rx));

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
            selected_tab: Tab::Home,
            focus_handle,
            home_tab: HomeTab::default(),
            logs_tab: LogsTab::default(),
            configuration_tab: ConfigurationTab::default(),
        }
    }

    fn tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(24.))
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE));

        for tab in Tab::ALL {
            let is_selected = tab == self.selected_tab;
            let label = if is_selected {
                format!("[{}]", tab.title())
            } else {
                format!(" {} ", tab.title())
            };
            let color = if is_selected {
                theme::fg()
            } else {
                theme::muted()
            };

            row = row.child(
                div()
                    .id(("tab", tab as usize))
                    .py(px(6.))
                    .px(px(4.))
                    .text_color(color)
                    .hover(|s| s.text_color(theme::fg()))
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_tab = tab;
                        cx.notify();
                    })),
            );
        }

        row
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
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(TITLEBAR_LEFT_INSET))
                    .child(self.tab_bar(cx)),
            )
    }
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
            Tab::Logs => self.logs_tab.render(),
            Tab::Configuration => self.configuration_tab.render(),
        };

        let body = div()
            .max_w(px(400.))
            .pl(px(32.))
            .pr(px(32.))
            .pb(px(32.))
            .pt(px(TITLEBAR_HEIGHT + 24.))
            .child(content);

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
    }
}
