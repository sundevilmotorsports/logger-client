mod configuration;
mod device_info;
mod home;
mod logs;

pub use configuration::ConfigurationTab;
pub use device_info::DeviceInfoTab;
pub use home::HomeTab;
pub use logs::LogsTab;

use gpui::{AnyElement, IntoElement, div, prelude::*};

use crate::theme::{self, FONT_SIZE};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Logs,
    Configuration,
    Info,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Home, Tab::Logs, Tab::Configuration, Tab::Info];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Logs => "Logs",
            Tab::Configuration => "Configuration",
            Tab::Info => "Info",
        }
    }
}

fn placeholder(text: &str) -> AnyElement {
    div()
        .font(theme::mono_font())
        .text_size(gpui::px(FONT_SIZE))
        .text_color(theme::muted())
        .child(text.to_string())
        .into_any_element()
}
