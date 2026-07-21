use gpui::AnyElement;

use super::placeholder;

#[derive(Default)]
pub struct LogsTab;

impl LogsTab {
    pub fn render(&self) -> AnyElement {
        placeholder("no logs yet")
    }
}
