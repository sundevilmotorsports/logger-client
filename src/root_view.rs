use pathfinder_color::ColorU;
use warpui::{
    elements::{
        ConstrainedBox, Container, CrossAxisAlignment, DispatchEventResult, EventHandler, Flex,
        Hoverable, MainAxisAlignment, MouseStateHandle, ParentElement, Rect, Stack, Text,
    },
    fonts::FamilyId,
    platform::Cursor,
    AppContext, Element, Entity, SingletonEntity as _, TypedActionView, View, ViewContext,
};

use crate::device::{self, DeviceState};

const BG: ColorU = ColorU { r: 13, g: 13, b: 15, a: 255 };
const FG: ColorU = ColorU { r: 200, g: 200, b: 210, a: 255 };
const MUTED: ColorU = ColorU { r: 80, g: 80, b: 95, a: 255 };
const GREEN: ColorU = ColorU { r: 80, g: 200, b: 120, a: 255 };
const AMBER: ColorU = ColorU { r: 200, g: 160, b: 60, a: 255 };
const RED: ColorU = ColorU { r: 200, g: 80, b: 80, a: 255 };

const FONT_SIZE: f32 = 13.;
const LABEL_COL: usize = 10;

fn subscribe<V, S>(
    ctx: &mut ViewContext<V>,
    mut rx: tokio::sync::watch::Receiver<S>,
    update: impl Fn(&mut V, &mut ViewContext<V>, S) + Send + Clone + 'static,
) where
    V: View + TypedActionView + Entity + 'static,
    S: Clone + Send + Sync + 'static,
{
    let spawner = ctx.spawner();
    ctx.spawn(
        async move {
            while rx.changed().await.is_ok() {
                let s = rx.borrow().clone();
                let f = update.clone();
                spawner.spawn(move |view, ctx| f(view, ctx, s)).await.ok();
            }
        },
        |_, _, _| {},
    );
}

fn format_uptime(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

pub struct RootView {
    state: DeviceState,
    font: FamilyId,
    req_tx: device::RequestTx,
    ping_hover: MouseStateHandle,
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

        let (tx, rx) = tokio::sync::watch::channel(DeviceState::default());
        let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || device::poll(tx, req_rx));
        subscribe(ctx, rx, Self::on_device_state);

        Self { state: DeviceState::default(), font, req_tx, ping_hover: Default::default() }
    }

    fn on_device_state(&mut self, ctx: &mut ViewContext<Self>, state: DeviceState) {
        let title = state.port.as_deref()
            .map(|p| format!("● {p}"))
            .unwrap_or_else(|| "logger-client".to_string());
        let wid = ctx.window_id();
        ctx.windows().set_window_title(wid, &title);
        self.state = state;
        ctx.notify();
    }

    fn row(&self, label: &str, value: &str, color: ColorU) -> Box<dyn Element> {
        let padded = format!("{label:<LABEL_COL$}");
        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .with_child(Text::new_inline(padded, self.font, FONT_SIZE).with_color(MUTED).finish())
            .with_child(Text::new_inline(value.to_string(), self.font, FONT_SIZE).with_color(color).finish())
            .finish()
    }
}

impl Entity for RootView {
    type Event = ();
}

impl View for RootView {
    fn ui_name() -> &'static str { "RootView" }

    fn render(&self, _: &AppContext) -> Box<dyn Element> {
        let (status, status_color) = match &self.state.port {
            Some(_) => ("connected", GREEN),
            None => ("scanning...", AMBER),
        };
        let port = self.state.port.as_deref().unwrap_or("—");
        let uptime = self.state.uptime.map(format_uptime).unwrap_or_else(|| "—".to_string());
        let (ping_str, ping_color) = match self.state.last_ping {
            Some(true) => ("ok", GREEN),
            Some(false) => ("error", RED),
            None => ("—", MUTED),
        };

        let req_tx = self.req_tx.clone();
        let font = self.font;
        
        let ping_btn = Hoverable::new(self.ping_hover.clone(), move |ms| {
            let color = if ms.is_hovered() { FG } else { MUTED };
            Text::new_inline("[ ping ]", font, FONT_SIZE).with_color(color).finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |_, _, _| { let _ = req_tx.send(device::Command::Ping); })
        .finish();

        let content = Flex::column()
            .with_spacing(4.)
            .with_child(Text::new_inline("logger-client", self.font, FONT_SIZE).with_color(MUTED).finish())
            .with_child(Text::new_inline("", self.font, FONT_SIZE).with_color(MUTED).finish())
            .with_child(self.row("status", status, status_color))
            .with_child(self.row("port", port, FG))
            .with_child(self.row("uptime", &uptime, FG))
            .with_child(Text::new_inline("", self.font, FONT_SIZE).with_color(MUTED).finish())
            .with_child(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(8.)
                    .with_child(ping_btn)
                    .with_child(Text::new_inline(ping_str, self.font, FONT_SIZE).with_color(ping_color).finish())
                    .finish(),
            )
            .finish();

        let ui = Stack::new()
            .with_child(Rect::new().with_background_color(BG).finish())
            .with_child(
                Container::new(ConstrainedBox::new(content).with_max_width(400.).finish())
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
