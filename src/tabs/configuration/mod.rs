mod commands;
mod model;
mod widgets;

use gpui::{
    AnyElement, AnyWindowHandle, Context, Hsla, IntoElement, SharedString, div, prelude::*, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::Input;
use gpui_component::label::Label;
use gpui_component::notification::NotificationType;
use gpui_component::spinner::Spinner;
use gpui_component::{Disableable, Sizable, WindowExt};

use crate::device;
use crate::root_view::RootView;
use crate::theme;
use commands::{load_from_file, push_to_device, refresh, save_to_file};
use model::{AdcChannelForm, CanDeviceForm, build_config_json, signal_count};
use widgets::{field_label, row_container, section_header};

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
    fn trigger_refresh(
        &mut self,
        log_tx: &device::LogRequestTx,
        window_handle: AnyWindowHandle,
        cx: &mut Context<RootView>,
    ) {
        if matches!(self.status, Status::Loading) {
            return;
        }
        self.status = Status::Loading;
        cx.notify();
        let log_tx = log_tx.clone();
        cx.spawn(async move |weak, cx| refresh(weak, cx, log_tx, window_handle).await)
            .detach();
    }
    
    pub(crate) fn auto_fetch(
        &mut self,
        log_tx: &device::LogRequestTx,
        window_handle: AnyWindowHandle,
        cx: &mut Context<RootView>,
    ) {
        if self.fetched_once {
            return;
        }
        self.fetched_once = true;
        self.trigger_refresh(log_tx, window_handle, cx);
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
            body = body.child(Label::new("No configuration loaded yet.").text_color(theme::muted()));
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
        let weak = cx.weak_entity();
        let refresh_tx = log_tx.clone();
        let refresh_btn = Button::new("config-refresh")
            .label("refresh")
            .small()
            .disabled(busy)
            .on_click(move |_, window, app| {
                let log_tx = refresh_tx.clone();
                let window_handle = window.window_handle();
                weak.update(app, |this, cx| {
                    this.configuration_tab
                        .trigger_refresh(&log_tx, window_handle, cx);
                })
                .ok();
            });

        let weak = cx.weak_entity();
        let load_tx = log_tx.clone();
        let load_btn = Button::new("config-load")
            .label("load file...")
            .small()
            .disabled(busy)
            .on_click(move |_, window, app| {
                let log_tx = load_tx.clone();
                let window_handle = window.window_handle();
                weak.update(app, |this, cx| {
                    if matches!(this.configuration_tab.status, Status::Loading) {
                        return;
                    }
                    this.configuration_tab.status = Status::Loading;
                    cx.notify();
                    cx.spawn(async move |weak, cx| {
                        load_from_file(weak, cx, log_tx, window_handle).await
                    })
                    .detach();
                })
                .ok();
            });

        let weak = cx.weak_entity();
        let save_tx = log_tx.clone();
        let save_btn = Button::new("config-save")
            .label("save file...")
            .small()
            .disabled(busy)
            .on_click(move |_, window, app| {
                let log_tx = save_tx.clone();
                let window_handle = window.window_handle();
                weak.update(app, |this, cx| {
                    if matches!(this.configuration_tab.status, Status::Loading) {
                        return;
                    }
                    this.configuration_tab.status = Status::Loading;
                    cx.notify();
                    cx.spawn(async move |weak, cx| {
                        save_to_file(weak, cx, log_tx, window_handle).await
                    })
                    .detach();
                })
                .ok();
            });

        let weak = cx.weak_entity();
        let push_tx = log_tx.clone();
        let push_btn = Button::new("config-push")
            .label("push to device")
            .primary()
            .small()
            .disabled(busy)
            .on_click(move |_, window, app| {
                let log_tx = push_tx.clone();
                let window_handle = window.window_handle();
                weak.update(app, |this, cx| {
                    if matches!(this.configuration_tab.status, Status::Loading) {
                        return;
                    }
                    let value = match build_config_json(&this.configuration_tab, cx) {
                        Ok(v) => v,
                        Err(e) => {
                            window.push_notification(
                                (
                                    NotificationType::Error,
                                    SharedString::from(format!("push failed: {e}")),
                                ),
                                cx,
                            );
                            return;
                        }
                    };
                    this.configuration_tab.status = Status::Loading;
                    cx.notify();
                    cx.spawn(async move |weak, cx| {
                        push_to_device(weak, cx, log_tx, value, window_handle).await
                    })
                    .detach();
                })
                .ok();
            });

        let mut status = div().flex().flex_row().items_center().gap(px(6.));
        if busy {
            status = status.child(Spinner::new().xsmall());
        }
        status = status.child(Label::new(status_text).text_color(status_color));

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(refresh_btn)
            .child(load_btn)
            .child(save_btn)
            .child(push_btn)
            .child(status)
            .into_any_element()
    }

    fn can_section(&self, cx: &mut Context<RootView>) -> AnyElement {
        let mut rows = div().flex().flex_col().gap(px(4.));
        for (i, d) in self.can_devices.iter().enumerate() {
            rows = rows.child(self.can_device_row(i, d, cx));
        }

        let weak = cx.weak_entity();
        let add_btn =
            Button::new("can-add")
                .label("+ add device")
                .ghost()
                .small()
                .on_click(move |_, window, app| {
                    weak.update(app, |this, cx| {
                        this.configuration_tab
                            .can_devices
                            .push(CanDeviceForm::new(window, cx));
                        cx.notify();
                    })
                    .ok();
                });

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

        let weak = cx.weak_entity();
        let remove_btn =
            Button::new(("can-remove", i))
                .label("remove")
                .ghost()
                .small()
                .on_click(move |_, _, app| {
                    weak.update(app, |this, cx| {
                        this.configuration_tab.can_devices.remove(i);
                        cx.notify();
                    })
                    .ok();
                });

        let weak = cx.weak_entity();
        let ext_checkbox =
            Checkbox::new(("can-ext", i))
                .checked(d.extended)
                .on_click(move |checked, _, app| {
                    let checked = *checked;
                    weak.update(app, |this, cx| {
                        this.configuration_tab.can_devices[i].extended = checked;
                        cx.notify();
                    })
                    .ok();
                });

        let weak = cx.weak_entity();
        let fd_checkbox =
            Checkbox::new(("can-fd", i))
                .checked(d.fd)
                .on_click(move |checked, _, app| {
                    let checked = *checked;
                    weak.update(app, |this, cx| {
                        this.configuration_tab.can_devices[i].fd = checked;
                        cx.notify();
                    })
                    .ok();
                });

        row_container()
            .child(field_label("id"))
            .child(Input::new(&d.id).w(px(90.)))
            .child(field_label("ext"))
            .child(ext_checkbox)
            .child(field_label("fd"))
            .child(fd_checkbox)
            .child(Label::new(format!("{signals} signal(s)")).text_color(theme::muted()))
            .child(remove_btn)
            .into_any_element()
    }

    fn adc_section(&self, cx: &mut Context<RootView>) -> AnyElement {
        let mut rows = div().flex().flex_col().gap(px(4.));
        for (i, c) in self.adc_channels.iter().enumerate() {
            rows = rows.child(self.adc_channel_row(i, c, cx));
        }

        let weak = cx.weak_entity();
        let add_btn =
            Button::new("adc-add")
                .label("+ add channel")
                .ghost()
                .small()
                .on_click(move |_, window, app| {
                    weak.update(app, |this, cx| {
                        this.configuration_tab
                            .adc_channels
                            .push(AdcChannelForm::new(window, cx));
                        cx.notify();
                    })
                    .ok();
                });

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
        let weak = cx.weak_entity();
        let remove_btn =
            Button::new(("adc-remove", i))
                .label("remove")
                .ghost()
                .small()
                .on_click(move |_, _, app| {
                    weak.update(app, |this, cx| {
                        this.configuration_tab.adc_channels.remove(i);
                        cx.notify();
                    })
                    .ok();
                });

        row_container()
            .child(field_label("name"))
            .child(Input::new(&c.name).w(px(110.)))
            .child(field_label("ch"))
            .child(Input::new(&c.channel).w(px(40.)))
            .child(field_label("scale"))
            .child(Input::new(&c.scale).w(px(70.)))
            .child(field_label("offset"))
            .child(Input::new(&c.offset).w(px(70.)))
            .child(remove_btn)
            .into_any_element()
    }
}
