use gpui::{AnyElement, IntoElement, prelude::*, px};
use gpui_component::{h_flex, v_flex};
use gpui_component::label::Label;

use crate::theme;

pub(super) fn section_header(title: &str, add_btn: impl IntoElement) -> AnyElement {
    h_flex()
        .justify_between()
        .child(Label::new(title.to_string()).text_color(theme::fg()))
        .child(add_btn)
        .into_any_element()
}

pub(super) fn row_container() -> gpui::Div {
    h_flex()
        .gap(px(8.))
        .px(px(8.))
        .py(px(4.))
        .rounded_sm()
        .bg(theme::panel_bg())
}

/// Stacks children vertically (each usually an `h_flex` line), indented under a device/group row.
pub(super) fn indented_col() -> gpui::Div {
    v_flex()
        .items_start()
        .gap(px(4.))
        .ml(px(20.))
        .px(px(8.))
        .py(px(4.))
        .rounded_sm()
        .bg(theme::panel_bg())
}

pub(super) fn field_label(text: &str) -> AnyElement {
    Label::new(text.to_string())
        .text_color(theme::muted())
        .into_any_element()
}

fn spec_row(field: &str, desc: &str) -> AnyElement {
    v_flex()
        .gap(px(1.))
        .child(Label::new(field.to_string()).text_color(theme::accent()))
        .child(Label::new(desc.to_string()).text_color(theme::muted()))
        .into_any_element()
}

fn help_group(title: &str, rows: Vec<AnyElement>) -> AnyElement {
    let mut col = v_flex()
        .gap(px(5.))
        .child(Label::new(title.to_string()).text_color(theme::fg()));
    for r in rows {
        col = col.child(r);
    }
    col.into_any_element()
}

fn mono_line(text: &str) -> AnyElement {
    Label::new(text.to_string())
        .text_color(theme::muted())
        .into_any_element()
}

/// Static reference card for the CAN section: what each field means, plus a
/// byte-layout diagram and a matching JSON example.
pub(super) fn can_help_panel() -> AnyElement {
    let diagram = v_flex()
        .gap(px(0.))
        .child(mono_line("┌──┬──┬──┬──┬──┬──┬──┬──┐"))
        .child(mono_line("│00│01│02│03│04│05│06│07│"))
        .child(mono_line("└──┴──┴──┴──┴──┴──┴──┴──┘"))
        .child(
            Label::new("      └──┬──┘")
                .text_color(theme::accent())
                .into_any_element(),
        )
        .child(
            Label::new("    start=2, len=2")
                .text_color(theme::accent())
                .into_any_element(),
        );

    let example = v_flex()
        .gap(px(0.))
        .px(px(6.))
        .py(px(4.))
        .rounded_sm()
        .bg(theme::bg())
        .child(mono_line("{ \"id\": \"0x100\", \"fd\": false,"))
        .child(mono_line("  \"signals\": { \"Fixed\": ["))
        .child(mono_line("    { \"name\": \"rpm\","))
        .child(mono_line("      \"start\": 2, \"len\": 2,"))
        .child(mono_line("      \"scale\": 0.25 }"))
        .child(mono_line("  ]}}"));

    v_flex()
        .id("can-help-panel")
        .overflow_y_scroll()
        .w(px(420.))
        .flex_shrink_0()
        .h_full()
        .gap(px(14.))
        .px(px(12.))
        .py(px(10.))
        .rounded_sm()
        .bg(theme::panel_bg())
        .border_1()
        .border_color(theme::border())
        .child(Label::new("CAN config reference").text_color(theme::fg()))
        .child(help_group(
            "device",
            vec![
                spec_row("id", "CAN arbitration id, hex (e.g. 0x100)"),
                spec_row("ext", "29-bit extended id vs 11-bit standard"),
                spec_row("fd", "CAN FD (up to 64B payload) vs classic CAN (8B)"),
            ],
        ))
        .child(
            v_flex()
                .gap(px(5.))
                .child(Label::new("signals: fixed").text_color(theme::fg()))
                .child(mono_line(
                    "same byte layout decoded on every frame with this id",
                ))
                .child(diagram),
        )
        .child(help_group(
            "signals: muxed",
            vec![
                mono_line(
                    "one byte (the discriminator) picks which signal group to decode",
                ),
                spec_row("byte", "index of the discriminator byte"),
                spec_row("type_val", "discriminator value that selects this group"),
            ],
        ))
        .child(help_group(
            "signal fields",
            vec![
                spec_row("name", "column name written to the log"),
                spec_row("start / len", "byte range read from the payload"),
                spec_row("signed", "treat raw bytes as two's complement"),
                spec_row("big-end", "MSB-first byte order (off = little-endian)"),
                spec_row(
                    "scale / offset",
                    "value = scale × (raw − offset); blank scale logs raw bytes",
                ),
            ],
        ))
        .child(help_group("example", vec![example.into_any_element()]))
        .into_any_element()
}
