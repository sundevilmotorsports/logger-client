mod configuration;
mod console;
mod device_info;
mod home;
mod logs;

pub use configuration::ConfigurationTab;
pub use console::ConsoleTab;
pub use device_info::DeviceInfoTab;
pub use home::HomeTab;
pub use logs::LogsTab;

use gpui::{AnyElement, prelude::*};
use gpui_component::label::Label;
use gpui_component::v_flex;

pub(crate) fn empty_state(text: &'static str) -> AnyElement {
    v_flex()
        .size_full()
        .flex_1()
        .items_center()
        .justify_center()
        .child(Label::new(text).text_color(crate::theme::muted()))
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Logs,
    Console,
    Configuration,
    Info,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Home,
        Tab::Logs,
        Tab::Console,
        Tab::Configuration,
        Tab::Info,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Logs => "Logs",
            Tab::Console => "Console",
            Tab::Configuration => "Configuration",
            Tab::Info => "Info",
        }
    }
}
