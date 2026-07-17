use warpui::{
    AppContext, Element, Entity, SingletonEntity as _, TypedActionView, View, ViewContext,
    elements::{
        ConstrainedBox, Container, CrossAxisAlignment, DispatchEventResult, EventHandler, Flex,
        Hoverable, MouseStateHandle, ParentElement, Rect, Stack, Text,
    },
    fonts::FamilyId,
    platform::Cursor,
};

use crate::device::{self, DeviceState};
use crate::tabs::{ConfigurationTab, HomeTab, LogsTab, Tab};
use crate::theme::{BG, FG, FONT_SIZE, MUTED, TITLEBAR_BG, TITLEBAR_HEIGHT, TITLEBAR_LEFT_INSET};

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

#[derive(Debug)]
pub enum RootViewAction {
    SelectTab(Tab),
}

pub struct RootView {
    state: DeviceState,
    font: FamilyId,
    req_tx: device::RequestTx,
    selected_tab: Tab,
    tab_hovers: [MouseStateHandle; Tab::ALL.len()],
    home_tab: HomeTab,
    logs_tab: LogsTab,
    configuration_tab: ConfigurationTab,
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

        Self {
            state: DeviceState::default(),
            font,
            req_tx,
            selected_tab: Tab::Home,
            tab_hovers: Default::default(),
            home_tab: HomeTab::default(),
            logs_tab: LogsTab::default(),
            configuration_tab: ConfigurationTab::default(),
        }
    }

    fn on_device_state(&mut self, ctx: &mut ViewContext<Self>, state: DeviceState) {
        let title = state
            .port
            .as_deref()
            .map(|p| format!("● {p}"))
            .unwrap_or_else(|| "Logger Client".to_string());
        let wid = ctx.window_id();
        ctx.windows().set_window_title(wid, &title);
        self.state = state;
        ctx.notify();
    }

    fn tab_bar(&self) -> Box<dyn Element> {
        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(24.);

        for (i, tab) in Tab::ALL.into_iter().enumerate() {
            let is_selected = tab == self.selected_tab;
            let label = if is_selected {
                format!("[{}]", tab.title())
            } else {
                format!(" {} ", tab.title().to_string())
            };
            let font = self.font;

            let btn = Hoverable::new(self.tab_hovers[i].clone(), move |ms| {
                let color = if is_selected || ms.is_hovered() {
                    FG
                } else {
                    MUTED
                };
                Container::new(
                    Text::new_inline(label.clone(), font, FONT_SIZE)
                        .with_color(color)
                        .finish(),
                )
                .with_vertical_padding(6.)
                .with_horizontal_padding(4.)
                .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(RootViewAction::SelectTab(tab));
            })
            .finish();

            row = row.with_child(btn);
        }

        row.finish()
    }

    /// A full-width strip behind the tabs, colored differently from the body
    fn title_bar(&self) -> Box<dyn Element> {
        ConstrainedBox::new(
            Stack::new()
                .with_child(Rect::new().with_background_color(TITLEBAR_BG).finish())
                .with_child(
                    Container::new(self.tab_bar())
                        .with_padding_left(TITLEBAR_LEFT_INSET)
                        .finish(),
                )
                .finish(),
        )
        .with_height(TITLEBAR_HEIGHT)
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
        let content = match self.selected_tab {
            Tab::Home => self.home_tab.render(&self.state, self.font, &self.req_tx),
            Tab::Logs => self.logs_tab.render(self.font),
            Tab::Configuration => self.configuration_tab.render(self.font),
        };

        let body = Container::new(ConstrainedBox::new(content).with_max_width(400.).finish())
            .with_padding_left(32.)
            .with_padding_right(32.)
            .with_padding_bottom(32.)
            .with_padding_top(TITLEBAR_HEIGHT + 24.)
            .finish();

        let ui = Stack::new()
            .with_child(Rect::new().with_background_color(BG).finish())
            .with_child(body)
            .with_child(self.title_bar())
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
    type Action = RootViewAction;

    fn handle_action(&mut self, action: &RootViewAction, ctx: &mut ViewContext<Self>) {
        let RootViewAction::SelectTab(tab) = action;
        self.selected_tab = *tab;
        ctx.notify();
    }
}
