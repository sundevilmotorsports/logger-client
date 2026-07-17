use warpui::{Element, fonts::FamilyId};

use super::placeholder;

#[derive(Default)]
pub struct LogsTab;

impl LogsTab {
    pub fn render(&self, font: FamilyId) -> Box<dyn Element> {
        placeholder(font, "no logs yet")
    }
}
