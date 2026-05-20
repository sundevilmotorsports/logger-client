use std::time::Duration;

use pathfinder_color::ColorU;
use warpui::{
    elements::{
        ConstrainedBox, CrossAxisAlignment, DispatchEventResult, EventHandler, Flex,
        MainAxisAlignment, ParentElement, Rect, Stack, Text,
    },
    fonts::FamilyId,
    AppContext, Element, Entity, SingletonEntity as _, TypedActionView, View, ViewContext,
};
use warpui::r#async::Timer;

use crate::device;

const BG: ColorU = ColorU { r: 13, g: 13, b: 15, a: 255 };
const FG: ColorU = ColorU { r: 200, g: 200, b: 210, a: 255 };
const MUTED: ColorU = ColorU { r: 80, g: 80, b: 95, a: 255 };
const GREEN: ColorU = ColorU { r: 80, g: 200, b: 120, a: 255 };
const AMBER: ColorU = ColorU { r: 200, g: 160, b: 60, a: 255 };

const FONT_SIZE: f32 = 13.;
const LABEL_COL: usize = 10;

// ── Components ──────────────────────────────────────────────────────────────

fn text(s: impl Into<String>, font: FamilyId, color: ColorU) -> Box<dyn Element> {
    Text::new_inline(s.into(), font, FONT_SIZE).with_color(color).finish()
}

fn info_row(
    label: &str,
    value: impl Into<String>,
    font: FamilyId,
    value_color: ColorU,
) -> Box<dyn Element> {
    let padded = format!("{:<width$}", label, width = LABEL_COL);
    Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_main_axis_alignment(MainAxisAlignment::Start)
        .with_child(text(padded, font, MUTED))
        .with_child(text(value, font, value_color))
        .finish()
}

fn format_uptime(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 { format!("{}:{:02}:{:02}", h, m, s) } else { format!("{}:{:02}", m, s) }
}

// ── View ─────────────────────────────────────────────────────────────────────

pub struct RootView {
    port: Option<String>,
    uptime: Option<u64>,
    font: FamilyId,
}

impl RootView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let font = warpui::fonts::Cache::handle(ctx).update(ctx, |cache, _| {
            cache
                .load_system_font("JetBrainsMono Nerd Font")
                .or_else(|_| cache.load_system_font("Hack"))
                .or_else(|_| cache.load_system_font("FreeMono"))
                .or_else(|_| cache.load_system_font("Menlo"))
                .or_else(|_| cache.load_system_font("Courier New"))
                .expect("no monospace font found")
        });

        let spawner = ctx.spawner();
        ctx.spawn(
            async move {
                loop {
                    match device::find_port() {
                        None => {
                            let _ = spawner
                                .spawn(|view: &mut RootView, ctx| {
                                    view.port = None;
                                    view.uptime = None;
                                    let wid = ctx.window_id();
                                    ctx.windows().set_window_title(wid, "logger-client");
                                    ctx.notify();
                                })
                                .await;
                            Timer::after(Duration::from_secs(3)).await;
                        }
                        Some(port) => {
                            let port_title = port.clone();
                            if spawner
                                .spawn(move |view: &mut RootView, ctx| {
                                    view.port = Some(port_title.clone());
                                    view.uptime = None;
                                    let wid = ctx.window_id();
                                    ctx.windows()
                                        .set_window_title(wid, &format!("● {}", port_title));
                                    ctx.notify();
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }

                            // Open the port once and keep it alive; reopening every second
                            // triggers the ESP32 auto-reset circuit via DTR.
                            let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<u64>>(4);
                            std::thread::spawn(move || device::serial_loop(port, tx));

                            while let Some(uptime) = rx.recv().await {
                                if spawner
                                    .spawn(move |view: &mut RootView, ctx| {
                                        view.uptime = uptime;
                                        ctx.notify();
                                    })
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                            // Thread exited (port error/disconnect) — rescan
                            Timer::after(Duration::from_secs(1)).await;
                        }
                    }
                }
            },
            |_, _, _| {},
        );

        Self { port: None, uptime: None, font }
    }
}

impl Entity for RootView {
    type Event = ();
}

impl View for RootView {
    fn ui_name() -> &'static str {
        "RootView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn Element> {
        let (status_str, status_color) = match &self.port {
            Some(_) => ("connected", GREEN),
            None => ("scanning...", AMBER),
        };
        let port_str = self.port.clone().unwrap_or_else(|| "—".to_string());
        let uptime_str = self.uptime.map(format_uptime).unwrap_or_else(|| "—".to_string());

        let content = Flex::column()
            .with_spacing(4.)
            .with_child(text("logger-client", self.font, MUTED))
            .with_child(text("", self.font, MUTED))
            .with_child(info_row("status", status_str, self.font, status_color))
            .with_child(info_row("port", port_str, self.font, FG))
            .with_child(info_row("uptime", uptime_str, self.font, FG))
            .finish();

        let ui = Stack::new()
            .with_child(Rect::new().with_background_color(BG).finish())
            .with_child(
                warpui::elements::Container::new(
                    ConstrainedBox::new(content).with_max_width(400.).finish(),
                )
                .with_uniform_padding(32.)
                .finish(),
            )
            .finish();

        EventHandler::new(ui)
            .on_keydown(|_ctx, _app, keystroke| {
                if keystroke.key == "q" && !keystroke.ctrl && !keystroke.cmd && !keystroke.alt {
                    std::process::exit(0);
                }
                DispatchEventResult::PropagateToParent
            })
            .finish()
    }
}

impl TypedActionView for RootView {
    type Action = ();
}
