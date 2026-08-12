mod commands;
mod model;
mod widgets;

use gpui::{AnyElement, AnyWindowHandle, Context, Entity, Hsla, IntoElement, div, prelude::*, px};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputState};
use gpui_component::label::Label;
use gpui_component::spinner::Spinner;
use gpui_component::{Disableable, Selectable, Sizable, h_flex, v_flex};

use crate::device;
use crate::root_view::RootView;
use crate::theme;
use crate::toast::ToastKind;
use commands::{load_from_file, push_to_device, refresh, save_to_file};
use model::{
    AdcChannelForm, CanDeviceForm, SignalForm, SignalGroupForm, SignalsForm, build_config_json,
    signal_count,
};
use widgets::{can_help_panel, field_label, indented_col, row_container, section_header};

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

        let body: AnyElement =
            if !self.loaded && self.can_devices.is_empty() && self.adc_channels.is_empty() {
                super::empty_state("no configuration loaded yet")
            } else {
                v_flex()
                    .gap(px(16.))
                    .child(self.can_section(cx))
                    .child(self.adc_section(cx))
                    .into_any_element()
            };

        v_flex()
            .font(theme::mono_font())
            .text_size(px(theme::FONT_SIZE))
            .size_full()
            .gap(px(12.))
            .child(self.toolbar(log_tx, busy, status_text, status_color, cx))
            .child(
                h_flex()
                    .w_full()
                    .flex_1()
                    .min_h(px(0.))
                    .gap(px(12.))
                    .child(
                        div()
                            .id("config-scroll")
                            .overflow_y_scroll()
                            .flex_1()
                            .max_w(px(640.))
                            .min_h(px(0.))
                            .child(body),
                    )
                    .child(can_help_panel()),
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
            .on_click(move |_, _, app| {
                let log_tx = push_tx.clone();
                weak.update(app, |this, cx| {
                    if matches!(this.configuration_tab.status, Status::Loading) {
                        return;
                    }
                    let value = match build_config_json(&this.configuration_tab, cx) {
                        Ok(v) => v,
                        Err(e) => {
                            this.push_toast(cx, format!("push failed: {e}"), ToastKind::Error);
                            return;
                        }
                    };
                    this.configuration_tab.status = Status::Loading;
                    cx.notify();
                    cx.spawn(async move |weak, cx| push_to_device(weak, cx, log_tx, value).await)
                        .detach();
                })
                .ok();
            });

        let mut status = h_flex().gap(px(6.));
        if busy {
            status = status.child(Spinner::new().xsmall());
        }
        status = status.child(Label::new(status_text).text_color(status_color));

        h_flex()
            .justify_between()
            .gap(px(8.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(refresh_btn)
                    .child(load_btn)
                    .child(save_btn),
            )
            .child(h_flex().gap(px(12.)).child(status).child(push_btn))
            .into_any_element()
    }

    fn can_section(&self, cx: &mut Context<RootView>) -> AnyElement {
        let mut rows = v_flex().gap(px(4.));
        for (i, d) in self.can_devices.iter().enumerate() {
            let mut block = v_flex().gap(px(2.)).child(self.can_device_row(i, d, cx));
            if d.expanded {
                block = block.child(self.signals_editor(i, d, cx));
            }
            rows = rows.child(block);
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

        v_flex()
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

        let weak = cx.weak_entity();
        let expand_btn = Button::new(("can-expand", i))
            .label(if d.expanded {
                format!("▾ {signals} signal(s)")
            } else {
                format!("▸ {signals} signal(s)")
            })
            .ghost()
            .small()
            .on_click(move |_, _, app| {
                weak.update(app, |this, cx| {
                    let dev = &mut this.configuration_tab.can_devices[i];
                    dev.expanded = !dev.expanded;
                    cx.notify();
                })
                .ok();
            });

        row_container()
            .child(field_label("id"))
            .child(Input::new(&d.id).w(px(90.)))
            .child(ext_checkbox.label("ext"))
            .child(fd_checkbox.label("fd"))
            .child(expand_btn)
            .child(remove_btn)
            .into_any_element()
    }

    fn signals_editor(&self, dev_i: usize, d: &CanDeviceForm, cx: &mut Context<RootView>) -> AnyElement {
        let is_muxed = matches!(d.signals, SignalsForm::Muxed { .. });

        let weak = cx.weak_entity();
        let fixed_mode_btn = Button::new(("sig-mode-fixed", dev_i))
            .label("Fixed")
            .small()
            .selected(!is_muxed)
            .on_click(move |_, _, app| {
                weak.update(app, |this, cx| {
                    this.configuration_tab.can_devices[dev_i].signals =
                        SignalsForm::Fixed(Vec::new());
                    cx.notify();
                })
                .ok();
            });
        let weak = cx.weak_entity();
        let muxed_mode_btn = Button::new(("sig-mode-muxed", dev_i))
            .label("Muxed")
            .small()
            .selected(is_muxed)
            .on_click(move |_, window, app| {
                weak.update(app, |this, cx| {
                    this.configuration_tab.can_devices[dev_i].signals =
                        SignalsForm::empty_muxed(window, cx);
                    cx.notify();
                })
                .ok();
            });

        let body = match &d.signals {
            SignalsForm::Fixed(sigs) => self.fixed_signals_body(dev_i, sigs, cx),
            SignalsForm::Muxed { byte, groups } => {
                self.muxed_signals_body(dev_i, byte, groups, cx)
            }
        };

        indented_col()
            .gap(px(6.))
            .child(h_flex().gap(px(6.)).child(fixed_mode_btn).child(muxed_mode_btn))
            .child(body)
            .into_any_element()
    }

    fn fixed_signals_body(
        &self,
        dev_i: usize,
        sigs: &[SignalForm],
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let mut rows = v_flex().gap(px(4.));
        for (i, s) in sigs.iter().enumerate() {
            rows = rows.child(self.signal_row(dev_i, None, i, s, cx));
        }

        let weak = cx.weak_entity();
        let add_btn = Button::new(("sig-add", dev_i))
            .label("+ signal")
            .ghost()
            .small()
            .on_click(move |_, window, app| {
                weak.update(app, |this, cx| {
                    if let SignalsForm::Fixed(sigs) =
                        &mut this.configuration_tab.can_devices[dev_i].signals
                    {
                        sigs.push(SignalForm::new(window, cx));
                    }
                    cx.notify();
                })
                .ok();
            });

        v_flex().gap(px(4.)).child(rows).child(add_btn).into_any_element()
    }

    fn muxed_signals_body(
        &self,
        dev_i: usize,
        byte: &Entity<InputState>,
        groups: &[SignalGroupForm],
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let mut rows = v_flex().gap(px(6.));
        for (gi, g) in groups.iter().enumerate() {
            rows = rows.child(self.muxed_group_row(dev_i, gi, g, cx));
        }

        let weak = cx.weak_entity();
        let add_group_btn = Button::new(("group-add", dev_i))
            .label("+ group")
            .ghost()
            .small()
            .on_click(move |_, window, app| {
                weak.update(app, |this, cx| {
                    if let SignalsForm::Muxed { groups, .. } =
                        &mut this.configuration_tab.can_devices[dev_i].signals
                    {
                        groups.push(SignalGroupForm {
                            type_val: cx
                                .new(|cx| InputState::new(window, cx).default_value("0")),
                            signals: Vec::new(),
                        });
                    }
                    cx.notify();
                })
                .ok();
            });

        v_flex()
            .gap(px(6.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(field_label("discriminator byte"))
                    .child(Input::new(byte).w(px(50.))),
            )
            .child(rows)
            .child(add_group_btn)
            .into_any_element()
    }

    fn muxed_group_row(
        &self,
        dev_i: usize,
        group_i: usize,
        g: &SignalGroupForm,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let weak = cx.weak_entity();
        let remove_group_btn = Button::new(("group-remove", dev_i * 1000 + group_i))
            .label("remove group")
            .ghost()
            .small()
            .on_click(move |_, _, app| {
                weak.update(app, |this, cx| {
                    if let SignalsForm::Muxed { groups, .. } =
                        &mut this.configuration_tab.can_devices[dev_i].signals
                    {
                        groups.remove(group_i);
                    }
                    cx.notify();
                })
                .ok();
            });

        let mut rows = v_flex().gap(px(4.));
        for (i, s) in g.signals.iter().enumerate() {
            rows = rows.child(self.signal_row(dev_i, Some(group_i), i, s, cx));
        }

        let weak = cx.weak_entity();
        let add_sig_btn = Button::new(("group-sig-add", dev_i * 1000 + group_i))
            .label("+ signal")
            .ghost()
            .small()
            .on_click(move |_, window, app| {
                weak.update(app, |this, cx| {
                    if let SignalsForm::Muxed { groups, .. } =
                        &mut this.configuration_tab.can_devices[dev_i].signals
                    {
                        groups[group_i].signals.push(SignalForm::new(window, cx));
                    }
                    cx.notify();
                })
                .ok();
            });

        indented_col()
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(field_label("type_val"))
                    .child(Input::new(&g.type_val).w(px(50.)))
                    .child(remove_group_btn),
            )
            .child(rows)
            .child(add_sig_btn)
            .into_any_element()
    }

    fn signal_row(
        &self,
        dev_i: usize,
        group_i: Option<usize>,
        sig_i: usize,
        s: &SignalForm,
        cx: &mut Context<RootView>,
    ) -> AnyElement {
        let key = match group_i {
            Some(gi) => dev_i * 1_000_000 + (gi + 1) * 1000 + sig_i,
            None => dev_i * 1_000_000 + sig_i,
        };

        let weak = cx.weak_entity();
        let signed_checkbox = Checkbox::new(("sig-signed", key))
            .checked(s.signed)
            .on_click(move |checked, _, app| {
                let checked = *checked;
                weak.update(app, |this, cx| {
                    set_signal_mut(&mut this.configuration_tab.can_devices, dev_i, group_i, sig_i, |s| {
                        s.signed = checked;
                    });
                    cx.notify();
                })
                .ok();
            });

        let weak = cx.weak_entity();
        let be_checkbox = Checkbox::new(("sig-be", key))
            .checked(s.big_endian)
            .on_click(move |checked, _, app| {
                let checked = *checked;
                weak.update(app, |this, cx| {
                    set_signal_mut(&mut this.configuration_tab.can_devices, dev_i, group_i, sig_i, |s| {
                        s.big_endian = checked;
                    });
                    cx.notify();
                })
                .ok();
            });

        let weak = cx.weak_entity();
        let remove_btn = Button::new(("sig-remove", key))
            .label("remove")
            .ghost()
            .small()
            .on_click(move |_, _, app| {
                weak.update(app, |this, cx| {
                    let dev = &mut this.configuration_tab.can_devices[dev_i];
                    match (&mut dev.signals, group_i) {
                        (SignalsForm::Fixed(sigs), None) => {
                            sigs.remove(sig_i);
                        }
                        (SignalsForm::Muxed { groups, .. }, Some(gi)) => {
                            groups[gi].signals.remove(sig_i);
                        }
                        _ => {}
                    }
                    cx.notify();
                })
                .ok();
            });

        indented_col()
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(field_label("name"))
                    .child(Input::new(&s.name).w(px(90.)))
                    .child(field_label("start"))
                    .child(Input::new(&s.start).w(px(40.)))
                    .child(field_label("len"))
                    .child(Input::new(&s.len).w(px(40.)))
                    .child(remove_btn),
            )
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(signed_checkbox.label("signed"))
                    .child(be_checkbox.label("big-end"))
                    .child(field_label("scale"))
                    .child(Input::new(&s.scale).w(px(60.)))
                    .child(field_label("offset"))
                    .child(Input::new(&s.offset).w(px(60.))),
            )
            .into_any_element()
    }

    fn adc_section(&self, cx: &mut Context<RootView>) -> AnyElement {
        let mut rows = v_flex().gap(px(4.));
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

        v_flex()
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

fn set_signal_mut(
    devices: &mut [CanDeviceForm],
    dev_i: usize,
    group_i: Option<usize>,
    sig_i: usize,
    f: impl FnOnce(&mut SignalForm),
) {
    let signals = &mut devices[dev_i].signals;
    let sig = match (signals, group_i) {
        (SignalsForm::Fixed(sigs), None) => &mut sigs[sig_i],
        (SignalsForm::Muxed { groups, .. }, Some(gi)) => &mut groups[gi].signals[sig_i],
        _ => return,
    };
    f(sig);
}
