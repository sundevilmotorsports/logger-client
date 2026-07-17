mod configuration;
mod home;
mod logs;

pub use configuration::ConfigurationTab;
pub use home::HomeTab;
pub use logs::LogsTab;

use warpui::{
    Element,
    elements::{Flex, ParentElement, Text},
    fonts::FamilyId,
};

use crate::theme::{FONT_SIZE, MUTED};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Logs,
    Configuration,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Home, Tab::Logs, Tab::Configuration];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Logs => "Logs",
            Tab::Configuration => "Configuration",
        }
    }
}

fn placeholder(font: FamilyId, text: &str) -> Box<dyn Element> {
    Flex::column()
        .with_child(
            Text::new_inline(text.to_string(), font, FONT_SIZE)
                .with_color(MUTED)
                .finish(),
        )
        .finish()
}
