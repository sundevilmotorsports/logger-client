pub mod configuration;
pub mod console;
pub mod devices;
pub mod home;
pub mod info;
pub mod logs;

pub use configuration::ConfigurationTab;
pub use console::ConsoleTab;
pub use devices::DevicesTab;
pub use logs::LogsTab;

use crate::app::model;
use crate::ui::{KvRow, Span, Tone};

pub(crate) fn span(text: impl Into<slint::SharedString>, tone: Tone) -> Span {
    Span {
        text: text.into(),
        tone,
    }
}

pub(crate) fn kv_row(label: &str, spans: Vec<Span>) -> KvRow {
    KvRow {
        label: label.into(),
        spans: model(spans),
    }
}
