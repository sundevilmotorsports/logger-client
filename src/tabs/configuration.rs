use warpui::{Element, fonts::FamilyId};

use super::placeholder;

#[derive(Default)]
pub struct ConfigurationTab;

impl ConfigurationTab {
    pub fn render(&self, font: FamilyId) -> Box<dyn Element> {
        placeholder(font, "no configuration yet")
    }
}
