use gpui::AnyElement;

use super::placeholder;

#[derive(Default)]
pub struct ConfigurationTab;

impl ConfigurationTab {
    pub fn render(&self) -> AnyElement {
        placeholder("no configuration yet")
    }
}
