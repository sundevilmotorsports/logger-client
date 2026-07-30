mod commands;
mod model;
mod widgets;

use gpui::{AnyElement, Context, Hsla, IntoElement, div, prelude::*, px};

use crate::device;
use crate::root_view::RootView;
use crate::theme;
use crate::toast::ToastKind;
use commands::{load_from_file, push_to_device, refresh, save_to_file};
use model::{AdcChannelForm, CanDeviceForm, build_config_json, signal_count};
use widgets::{checkbox, field_label, row_container, section_header, text_field};

#[derive(Default)]
enum Status {
    #[default]
    Idle,
    Loading,
    Error(String),
}

#[derive(Default)]
pub struct ConfigurationTab {
    status: Status,
    can_devices: Vec<CanDeviceForm>,
    adc_channels: Vec<AdcChannelForm>,
    loaded: bool,
    fetched_once: bool,
}

impl ConfigurationTab {
    fn trigger_refresh(&mut self, log_tx: &device::LogRequestTx, cx: &mut Context<RootView>) {
        if matches!(self.status, Status::Loading) {
            return;
        }
        self.status = Status::Loading;
        cx.notify();
        let log_tx = log_tx.clone();
        cx.spawn(async move |weak, cx| refresh(weak, cx, log_tx).await)
            .detach();
    }
    
    pub(crate) fn auto_fetch(&mut self, log_tx: &device::LogRequestTx, cx: &mut Context<RootView>) {
        if self.fetched_once {
            return;
        }
        self.fetched_once = true;
        self.trigger_refresh(log_tx, cx);
    }

    pub fn render(&self, log_tx: &device::LogRequestTx, cx: &mut Context<RootView>) -> AnyElement {
        let busy = matches!(self.status, Status::Loading);
        let (status_text, status_color) = match &self.status {
            Status::Idle if !self.loaded => ("not loaded".to_string(), theme::muted()),
            Status::Idle => ("in sync with last fetch/push".to_string(), theme::green()),
            Status::Loading => ("working...".to_string(), theme::amber()),
            Status::Error(e) => (format!("error: {e}"), theme::red()),
        };

        let mut body = div().flex().flex_col().gap(px(16.));

        if !self.loaded && self.can_devices.is_empty() && self.adc_channels.is_empty() {
            body = body.child(
                div()
                    .text_color(theme::muted())
                    .child("No configuration loaded yet."),
            );
        } else {
            body = body.child(self.can_section(cx)).child(self.adc_section(cx));
        }

        div()
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .size_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(self.toolbar(log_tx, busy, status_text, status_color, cx))
            .child(
                div()
                    .id("config-scroll")
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h(px(0.))
                    .child(body),
            )
            .into_any_element()
    }

    fn toolbar(
        &self,
        log_tx: &device::LogRequestTx,
        busy: bool,
        status_text: String,
        status_color: Hsla,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let refresh_tx = log_tx.clone();
        let refresh_btn = theme::button("config-refresh", "refresh").on_click(cx.listener(
            move |this, _, _, cx| {
                let log_tx = refresh_tx.clone();
                this.configuration_tab.trigger_refresh(&log_tx, cx);
            },
        ));

        let load_tx = log_tx.clone();
        let load_btn = theme::button("config-load", "load file...").on_click(cx.listener(
            move |this, _, _, cx| {
                if busy {
                    return;
                }
                this.configuration_tab.status = Status::Loading;
                cx.notify();
                let log_tx = load_tx.clone();
                cx.spawn(async move |weak, cx| load_from_file(weak, cx, log_tx).await)
                    .detach();
            },
        ));

        let save_tx = log_tx.clone();
        let save_btn = theme::button("config-save", "save file...").on_click(cx.listener(
            move |this, _, _, cx| {
                if busy {
                    return;
                }
                this.configuration_tab.status = Status::Loading;
                cx.notify();
                let log_tx = save_tx.clone();
                cx.spawn(async move |weak, cx| save_to_file(weak, cx, log_tx).await)
                    .detach();
            },
        ));

        let push_tx = log_tx.clone();
        let push_btn = theme::button("config-push", "push to device").on_click(cx.listener(
            move |this, _, _, cx| {
                if busy {
                    return;
                }
                let value = match build_config_json(&this.configuration_tab) {
                    Ok(v) => v,
                    Err(e) => {
                        this.push_toast(cx, format!("push failed: {e}"), ToastKind::Error);
                        return;
                    }
                };
                this.configuration_tab.status = Status::Loading;
                cx.notify();
                let log_tx = push_tx.clone();
                cx.spawn(async move |weak, cx| push_to_device(weak, cx, log_tx, value).await)
                    .detach();
            },
        ));

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(refresh_btn)
            .child(load_btn)
            .child(save_btn)
            .child(push_btn)
            .child(div().text_color(status_color).child(status_text))
            .into_any_element()
    }

    fn can_section(&self, cx: &mut Context<RootView>) -> AnyElement {
        let mut rows = div().flex().flex_col().gap(px(4.));
        for (i, d) in self.can_devices.iter().enumerate() {
            rows = rows.child(self.can_device_row(i, d, cx));
        }

        let add_btn =
            theme::button("can-add", "+ add device").on_click(cx.listener(|this, _, _, cx| {
                this.configuration_tab
                    .can_devices
                    .push(CanDeviceForm::new(cx));
                cx.notify();
            }));

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(section_header("CAN devices", add_btn))
            .child(rows)
            .into_any_element()
    }

    fn can_device_row(
        &self,
        i: usize,
        d: &CanDeviceForm,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let signals = signal_count(&d.signals);
        let remove_btn = theme::button(("can-remove", i), "remove").on_click(cx.listener(
            move |this, _, _, cx| {
                this.configuration_tab.can_devices.remove(i);
                cx.notify();
            },
        ));

        row_container()
            .child(field_label("id"))
            .child(text_field(
                ("can-id", i),
                &d.id,
                90.,
                &d.id_focus,
                move |tab| &mut tab.can_devices[i].id,
                cx,
            ))
            .child(field_label("ext"))
            .child(checkbox(("can-ext", i), d.extended, cx, move |this, _| {
                this.configuration_tab.can_devices[i].extended =
                    !this.configuration_tab.can_devices[i].extended;
            }))
            .child(field_label("fd"))
            .child(checkbox(("can-fd", i), d.fd, cx, move |this, _| {
                this.configuration_tab.can_devices[i].fd =
                    !this.configuration_tab.can_devices[i].fd;
            }))
            .child(
                div()
                    .text_color(theme::muted())
                    .child(format!("{signals} signal(s)")),
            )
            .child(remove_btn)
            .into_any_element()
    }

    fn adc_section(&self, cx: &mut Context<RootView>) -> AnyElement {
        let mut rows = div().flex().flex_col().gap(px(4.));
        for (i, c) in self.adc_channels.iter().enumerate() {
            rows = rows.child(self.adc_channel_row(i, c, cx));
        }

        let add_btn =
            theme::button("adc-add", "+ add channel").on_click(cx.listener(|this, _, _, cx| {
                this.configuration_tab
                    .adc_channels
                    .push(AdcChannelForm::new(cx));
                cx.notify();
            }));

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(section_header("ADC channels", add_btn))
            .child(rows)
            .into_any_element()
    }

    fn adc_channel_row(
        &self,
        i: usize,
        c: &AdcChannelForm,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let remove_btn = theme::button(("adc-remove", i), "remove").on_click(cx.listener(
            move |this, _, _, cx| {
                this.configuration_tab.adc_channels.remove(i);
                cx.notify();
            },
        ));

        row_container()
            .child(field_label("name"))
            .child(text_field(
                ("adc-name", i),
                &c.name,
                110.,
                &c.name_focus,
                move |tab| &mut tab.adc_channels[i].name,
                cx,
            ))
            .child(field_label("ch"))
            .child(text_field(
                ("adc-channel", i),
                &c.channel,
                40.,
                &c.channel_focus,
                move |tab| &mut tab.adc_channels[i].channel,
                cx,
            ))
            .child(field_label("scale"))
            .child(text_field(
                ("adc-scale", i),
                &c.scale,
                70.,
                &c.scale_focus,
                move |tab| &mut tab.adc_channels[i].scale,
                cx,
            ))
            .child(field_label("offset"))
            .child(text_field(
                ("adc-offset", i),
                &c.offset,
                70.,
                &c.offset_focus,
                move |tab| &mut tab.adc_channels[i].offset,
                cx,
            ))
            .child(remove_btn)
            .into_any_element()
    }
}
