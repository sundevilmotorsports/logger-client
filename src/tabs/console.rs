use gpui::{
    AnyElement, Context, IntoElement, ListSizingBehavior, Pixels, Task, UniformListScrollHandle,
    div, prelude::*, px, uniform_list,
};
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use gpui_component::spinner::Spinner;
use gpui_component::{Sizable, h_flex, v_flex};
use std::sync::Arc;
use std::time::Duration;

use crate::console;
use crate::root_view::RootView;
use crate::theme;

const POLL_INTERVAL: Duration = Duration::from_millis(350);
const ROW_HEIGHT: Pixels = px(20.);

pub struct ConsoleTab {
    connected_port: Option<String>,
    lines: Arc<Vec<String>>,
    last_line_count: usize,
    scroll_handle: UniformListScrollHandle,
    task: Option<Task<()>>,
}

impl Default for ConsoleTab {
    fn default() -> Self {
        Self {
            connected_port: None,
            lines: Arc::new(Vec::new()),
            last_line_count: 0,
            scroll_handle: UniformListScrollHandle::default(),
            task: None,
        }
    }
}

impl ConsoleTab {
    fn set_lines(&mut self, lines: Arc<Vec<String>>) {
        self.lines = lines;
    }

    pub(crate) fn start(&mut self, cx: &mut Context<RootView>) {
        if self.task.is_some() {
            return;
        }
        self.task = Some(cx.spawn(async move |weak, cx| {
            loop {
                let port_name = loop {
                    if let Some(name) = console::find_port() {
                        break name;
                    }
                    cx.background_executor()
                        .timer(Duration::from_secs(1))
                        .await;
                };

                let port = match console::open(&port_name) {
                    Ok(port) => port,
                    Err(e) => {
                        log::warn!("console: failed to open {port_name}: {e}");
                        cx.background_executor()
                            .timer(Duration::from_secs(1))
                            .await;
                        continue;
                    }
                };

                let shared = console::Lines::default();
                let handle = console::spawn_reader(port, shared.clone());

                let alive = weak
                    .update(cx, |view, cx| {
                        view.console_tab.connected_port = Some(port_name.clone());
                        view.console_tab.lines = Arc::new(Vec::new());
                        view.console_tab.last_line_count = 0;
                        cx.notify();
                    })
                    .is_ok();
                if !alive {
                    return;
                }

                let mut last_seen_len = 0usize;
                loop {
                    cx.background_executor().timer(POLL_INTERVAL).await;
                    if handle.is_finished() {
                        break;
                    }
                    let len = shared.len();
                    if len == last_seen_len {
                        continue;
                    }
                    last_seen_len = len;
                    let snapshot: Arc<Vec<String>> = Arc::new(shared.snapshot().into());
                    let alive = weak
                        .update(cx, |view, cx| {
                            view.console_tab.set_lines(snapshot);
                            cx.notify();
                        })
                        .is_ok();
                    if !alive {
                        return;
                    }
                }

                // Reader exited: device unplugged or port closed. Report
                // disconnected and loop back to scanning.
                let alive = weak
                    .update(cx, |view, cx| {
                        view.console_tab.connected_port = None;
                        cx.notify();
                    })
                    .is_ok();
                if !alive {
                    return;
                }
            }
        }));
    }

    pub fn render(&mut self, _cx: &mut Context<RootView>) -> AnyElement {
        match self.connected_port.clone() {
            Some(port) => self.log_view(port),
            None => self.waiting_view(),
        }
    }

    fn waiting_view(&self) -> AnyElement {
        v_flex()
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .child(Spinner::new().small())
            .child(Label::new("waiting for device...").text_color(theme::muted()))
            .into_any_element()
    }

    fn log_view(&mut self, port: String) -> AnyElement {
        let count = self.lines.len();
        let grew = count > self.last_line_count;
        self.last_line_count = count;

        if grew && is_near_bottom(&self.scroll_handle) {
            scroll_to_bottom(&self.scroll_handle);
        }

        let lines = self.lines.clone();
        let scroll_handle = self.scroll_handle.clone();

        v_flex()
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .size_full()
            .gap(px(8.))
            .child(
                h_flex()
                    .gap(px(6.))
                    .child(div().text_color(theme::green()).child("●"))
                    .child(
                        Label::new(format!("connected: {port}")).text_color(theme::muted()),
                    ),
            )
            .child(
                div()
                    .id("console-log")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .rounded_md()
                    .border_1()
                    .border_color(theme::border())
                    .vertical_scrollbar(&self.scroll_handle)
                    .child(
                        uniform_list("console-lines", count, move |range, _window, _cx| {
                            range
                                .map(|ix| {
                                    div()
                                        .h(ROW_HEIGHT)
                                        .px(px(8.))
                                        .flex()
                                        .items_center()
                                        .overflow_hidden()
                                        .child(
                                            Label::new(lines[ix].clone())
                                                .text_color(theme::fg())
                                                .whitespace_nowrap(),
                                        )
                                })
                                .collect::<Vec<_>>()
                        })
                        .size_full()
                        .with_sizing_behavior(ListSizingBehavior::Auto)
                        .track_scroll(scroll_handle)
                        .into_any_element(),
                    ),
            )
            .into_any_element()
    }
}

fn is_near_bottom(handle: &UniformListScrollHandle) -> bool {
    let state = handle.0.borrow();
    let offset = state.base_handle.offset();
    let max_offset = state.base_handle.max_offset();
    (max_offset.height + offset.y) <= px(4.)
}

fn scroll_to_bottom(handle: &UniformListScrollHandle) {
    handle.0.borrow().base_handle.scroll_to_bottom();
}
