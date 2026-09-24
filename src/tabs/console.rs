use slint::ComponentHandle;
use std::rc::Rc;
use std::time::Duration;

use crate::app::{self, App, model};
use crate::console;
use crate::ui::{AppWindow, ConsoleLine, ConsoleState, Tone};

const POLL_INTERVAL: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn parse(line: &str) -> Option<Level> {
        let mut chars = line.chars();
        let letter = chars.next()?;
        if chars.next() != Some(' ') || chars.next() != Some('(') {
            return None;
        }
        match letter {
            'E' => Some(Level::Error),
            'W' => Some(Level::Warn),
            'I' => Some(Level::Info),
            'D' => Some(Level::Debug),
            _ => None,
        }
    }

    fn tone(self) -> Tone {
        match self {
            Level::Error => Tone::Red,
            Level::Warn => Tone::Amber,
            Level::Info => Tone::Fg,
            Level::Debug => Tone::Muted,
        }
    }
}

fn line_tone(line: &str) -> Tone {
    Level::parse(line).map_or(Tone::Fg, Level::tone)
}

/// Order matches the filter `TabBar` in `ui/tabs/console.slint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LevelFilter {
    #[default]
    All,
    Debug,
    Info,
    Warn,
    Error,
}

impl LevelFilter {
    const ALL: [LevelFilter; 5] = [
        LevelFilter::All,
        LevelFilter::Debug,
        LevelFilter::Info,
        LevelFilter::Warn,
        LevelFilter::Error,
    ];

    fn min_level(self) -> Option<Level> {
        match self {
            LevelFilter::All => None,
            LevelFilter::Debug => Some(Level::Debug),
            LevelFilter::Info => Some(Level::Info),
            LevelFilter::Warn => Some(Level::Warn),
            LevelFilter::Error => Some(Level::Error),
        }
    }

    /// Lines whose level cant be parsed only show under all
    fn matches(self, line: &str) -> bool {
        match self.min_level() {
            None => true,
            Some(min) => Level::parse(line).is_some_and(|lvl| lvl >= min),
        }
    }
}

#[derive(Default)]
pub struct ConsoleTab {
    connected_port: Option<String>,
    lines: Vec<String>,
    filter: LevelFilter,
}

pub fn wire(app: &Rc<App>, ui: &AppWindow) {
    let app = app.clone();
    ui.global::<ConsoleState>().on_set_filter(move |ix| {
        if let Some(filter) = LevelFilter::ALL.get(ix as usize) {
            app.console_tab.borrow_mut().filter = *filter;
            app.render_console();
        }
    });
}

impl ConsoleTab {
    /// Scans for the ESP32's USB-JTAG console port forever, streaming its
    /// output while connected.
    pub(crate) fn start(app: &Rc<App>) {
        let app = app.clone();
        app::spawn(async move {
            loop {
                let port_name = loop {
                    if let Some(name) = console::find_port() {
                        break name;
                    }
                    app::sleep(Duration::from_secs(1)).await;
                };

                let port = match console::open(&port_name) {
                    Ok(port) => port,
                    Err(e) => {
                        log::warn!("console: failed to open {port_name}: {e}");
                        app::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };

                let shared = console::Lines::default();
                let handle = console::spawn_reader(port, shared.clone());

                {
                    let mut tab = app.console_tab.borrow_mut();
                    tab.connected_port = Some(port_name);
                    tab.lines.clear();
                }
                app.render_console();

                let mut last_seen_len = 0usize;
                loop {
                    app::sleep(POLL_INTERVAL).await;
                    if handle.is_finished() {
                        break;
                    }
                    let len = shared.len();
                    if len == last_seen_len {
                        continue;
                    }
                    last_seen_len = len;
                    app.console_tab.borrow_mut().lines = shared.snapshot().into();
                    app.render_console();
                }

                // Reader exited: device unplugged or port closed. Report
                // disconnected and loop back to scanning.
                app.console_tab.borrow_mut().connected_port = None;
                app.render_console();
            }
        });
    }

    pub fn render(&self, ui: &AppWindow) {
        let lines: Vec<ConsoleLine> = self
            .lines
            .iter()
            .filter(|line| self.filter.matches(line))
            .map(|line| ConsoleLine {
                text: line.into(),
                tone: line_tone(line),
            })
            .collect();

        let s = ui.global::<ConsoleState>();
        s.set_port(self.connected_port.clone().unwrap_or_default().into());
        s.set_lines(model(lines));
        s.set_filter(
            LevelFilter::ALL
                .iter()
                .position(|f| *f == self.filter)
                .unwrap_or(0) as i32,
        );
    }
}
