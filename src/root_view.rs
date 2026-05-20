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

const BG: ColorU = ColorU { r: 13, g: 13, b: 15, a: 255 };
const FG: ColorU = ColorU { r: 200, g: 200, b: 210, a: 255 };
const MUTED: ColorU = ColorU { r: 80, g: 80, b: 95, a: 255 };
const GREEN: ColorU = ColorU { r: 80, g: 200, b: 120, a: 255 };
const AMBER: ColorU = ColorU { r: 200, g: 160, b: 60, a: 255 };

const FONT_SIZE: f32 = 13.;
const LABEL_COL: usize = 10;

fn find_usb_serial_port() -> Option<String> {
    let ports = serialport::available_ports().ok()?;
    ports.into_iter().find_map(|p| match p.port_type {
        serialport::SerialPortType::UsbPort(_) => Some(p.port_name),
        _ => None,
    })
}

pub struct RootView {
    port: Option<String>,
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
                    let port = find_usb_serial_port();
                    if spawner
                        .spawn(move |view: &mut RootView, ctx| {
                            view.port = port;
                            ctx.notify();
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                    Timer::after(Duration::from_secs(3)).await;
                }
            },
            |_, _, _| {},
        );

        Self { port: None, font }
    }

    fn text(&self, s: impl Into<String>, color: ColorU) -> Box<dyn Element> {
        Text::new_inline(s.into(), self.font, FONT_SIZE)
            .with_color(color)
            .finish()
    }

    fn row(&self, label: &str, value: impl Into<String>, value_color: ColorU) -> Box<dyn Element> {
        let padded = format!("{:<width$}", label, width = LABEL_COL);
        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .with_child(self.text(padded, MUTED))
            .with_child(self.text(value, value_color))
            .finish()
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

        let content = Flex::column()
            .with_spacing(4.)
            .with_child(self.text("logger-client", MUTED))
            .with_child(self.text("", MUTED))
            .with_child(self.row("status", status_str, status_color))
            .with_child(self.row("port", port_str, FG))
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
