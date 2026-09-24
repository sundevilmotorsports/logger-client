mod commands;
mod model;

use slint::ComponentHandle;
use std::rc::Rc;

use crate::app::{self, App, ToastKind};
use crate::ui::{AppWindow, ConfigState, Tone};
use commands::{load_from_file, push_to_device, refresh, save_to_file};
use model::{
    AdcChannelForm, CanDeviceForm, SignalForm, SignalGroupForm, SignalsForm,
    adc_channels_from_json, build_config_json, can_devices_from_json,
};

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
    fn load(&mut self, v: &serde_json::Value) {
        self.can_devices = can_devices_from_json(v);
        self.adc_channels = adc_channels_from_json(v);
        self.loaded = true;
        self.status = Status::Idle;
    }

    /// Marks the tab busy, returning false if it already was.
    fn begin(&mut self) -> bool {
        if matches!(self.status, Status::Loading) {
            return false;
        }
        self.status = Status::Loading;
        true
    }

    fn trigger_refresh(app: &Rc<App>) {
        if !app.configuration_tab.borrow_mut().begin() {
            return;
        }
        app.render_configuration();
        let app = app.clone();
        app::spawn(async move { refresh(&app).await });
    }

    pub(crate) fn auto_fetch(app: &Rc<App>) {
        {
            let mut tab = app.configuration_tab.borrow_mut();
            if tab.fetched_once {
                return;
            }
            tab.fetched_once = true;
        }
        Self::trigger_refresh(app);
    }

    pub fn render(&self, ui: &AppWindow) {
        let (status_text, status_tone) = match &self.status {
            Status::Idle if !self.loaded => ("not loaded".to_string(), Tone::Muted),
            Status::Idle => ("in sync with last fetch/push".to_string(), Tone::Green),
            Status::Loading => ("working...".to_string(), Tone::Amber),
            Status::Error(e) => (format!("error: {e}"), Tone::Red),
        };

        let s = ui.global::<ConfigState>();
        s.set_show_form(
            self.loaded || !self.can_devices.is_empty() || !self.adc_channels.is_empty(),
        );
        s.set_busy(matches!(self.status, Status::Loading));
        s.set_status_text(status_text.into());
        s.set_status_tone(status_tone);
        s.set_can_devices(app::model(
            self.can_devices.iter().map(CanDeviceForm::to_ui).collect(),
        ));
        s.set_adc_channels(app::model(
            self.adc_channels.iter().map(AdcChannelForm::to_ui).collect(),
        ));
    }
}

/// Slint passes -1 for "no group" (fixed signals).
fn group_index(group: i32) -> Option<usize> {
    usize::try_from(group).ok()
}

pub fn wire(app: &Rc<App>, ui: &AppWindow) {
    let s = ui.global::<ConfigState>();

    // Edits the tab, then re-renders it. For structural changes (add, remove,
    // expand); plain field edits skip the re-render so a text field being
    // typed into isn't rebuilt under the cursor.
    fn edit(app: &Rc<App>, f: impl FnOnce(&mut ConfigurationTab)) {
        f(&mut app.configuration_tab.borrow_mut());
        app.render_configuration();
    }
    fn device(app: &App, dev: i32, f: impl FnOnce(&mut CanDeviceForm)) {
        if let Some(d) = app
            .configuration_tab
            .borrow_mut()
            .can_devices
            .get_mut(dev as usize)
        {
            f(d);
        }
    }
    fn signal(app: &App, dev: i32, group: i32, sig: i32, f: impl FnOnce(&mut SignalForm)) {
        device(app, dev, |d| {
            if let Some(s) = d
                .signal_list_mut(group_index(group))
                .and_then(|sigs| sigs.get_mut(sig as usize))
            {
                f(s);
            }
        });
    }

    let a = app.clone();
    s.on_refresh(move || ConfigurationTab::trigger_refresh(&a));

    let a = app.clone();
    s.on_load_file(move || {
        if !a.configuration_tab.borrow_mut().begin() {
            return;
        }
        a.render_configuration();
        let a = a.clone();
        app::spawn(async move { load_from_file(&a).await });
    });

    let a = app.clone();
    s.on_save_file(move || {
        if !a.configuration_tab.borrow_mut().begin() {
            return;
        }
        a.render_configuration();
        let a = a.clone();
        app::spawn(async move { save_to_file(&a).await });
    });

    let a = app.clone();
    s.on_push(move || {
        let value = {
            let tab = a.configuration_tab.borrow();
            if matches!(tab.status, Status::Loading) {
                return;
            }
            build_config_json(&tab)
        };
        let value = match value {
            Ok(v) => v,
            Err(e) => {
                a.push_toast(format!("push failed: {e}"), ToastKind::Error);
                return;
            }
        };
        a.configuration_tab.borrow_mut().status = Status::Loading;
        a.render_configuration();
        let a = a.clone();
        app::spawn(async move { push_to_device(&a, value).await });
    });

    // CAN devices

    let a = app.clone();
    s.on_can_add(move || edit(&a, |t| t.can_devices.push(CanDeviceForm::new())));

    let a = app.clone();
    s.on_can_remove(move |dev| {
        edit(&a, |t| {
            if (dev as usize) < t.can_devices.len() {
                t.can_devices.remove(dev as usize);
            }
        })
    });

    let a = app.clone();
    s.on_can_id_edited(move |dev, text| device(&a, dev, |d| d.id = text.into()));

    let a = app.clone();
    s.on_can_toggled(move |dev, field, checked| {
        device(&a, dev, |d| match field.as_str() {
            "extended" => d.extended = checked,
            "fd" => d.fd = checked,
            "engine" => d.engine = checked,
            _ => {}
        })
    });

    let a = app.clone();
    s.on_can_expand(move |dev| {
        device(&a, dev, |d| d.expanded = !d.expanded);
        a.render_configuration();
    });

    let a = app.clone();
    s.on_set_muxed(move |dev, muxed| {
        device(&a, dev, |d| {
            match (&d.signals, muxed) {
                (SignalsForm::Fixed(_), true) => d.signals = SignalsForm::empty_muxed(),
                (SignalsForm::Muxed { .. }, false) => d.signals = SignalsForm::Fixed(Vec::new()),
                _ => {}
            }
        });
        a.render_configuration();
    });

    let a = app.clone();
    s.on_mux_byte_edited(move |dev, text| {
        device(&a, dev, |d| {
            if let SignalsForm::Muxed { byte, .. } = &mut d.signals {
                *byte = text.into();
            }
        })
    });

    // Muxed signal groups

    let a = app.clone();
    s.on_group_add(move |dev| {
        device(&a, dev, |d| {
            if let SignalsForm::Muxed { groups, .. } = &mut d.signals {
                groups.push(SignalGroupForm {
                    type_val: "0".to_string(),
                    signals: Vec::new(),
                });
            }
        });
        a.render_configuration();
    });

    let a = app.clone();
    s.on_group_remove(move |dev, group| {
        device(&a, dev, |d| {
            if let SignalsForm::Muxed { groups, .. } = &mut d.signals
                && (group as usize) < groups.len()
            {
                groups.remove(group as usize);
            }
        });
        a.render_configuration();
    });

    let a = app.clone();
    s.on_group_type_edited(move |dev, group, text| {
        device(&a, dev, |d| {
            if let SignalsForm::Muxed { groups, .. } = &mut d.signals
                && let Some(g) = groups.get_mut(group as usize)
            {
                g.type_val = text.into();
            }
        })
    });

    // Signals

    let a = app.clone();
    s.on_signal_add(move |dev, group| {
        device(&a, dev, |d| {
            if let Some(sigs) = d.signal_list_mut(group_index(group)) {
                sigs.push(SignalForm::new());
            }
        });
        a.render_configuration();
    });

    let a = app.clone();
    s.on_signal_remove(move |dev, group, sig| {
        device(&a, dev, |d| {
            if let Some(sigs) = d.signal_list_mut(group_index(group))
                && (sig as usize) < sigs.len()
            {
                sigs.remove(sig as usize);
            }
        });
        a.render_configuration();
    });

    let a = app.clone();
    s.on_signal_edited(move |dev, group, sig, field, text| {
        signal(&a, dev, group, sig, |s| {
            let text = text.to_string();
            match field.as_str() {
                "name" => s.name = text,
                "start" => s.start = text,
                "len" => s.len = text,
                "scale" => s.scale = text,
                "offset" => s.offset = text,
                _ => {}
            }
        })
    });

    let a = app.clone();
    s.on_signal_toggled(move |dev, group, sig, field, checked| {
        signal(&a, dev, group, sig, |s| match field.as_str() {
            "signed" => s.signed = checked,
            "big_endian" => s.big_endian = checked,
            _ => {}
        })
    });

    // ADC channels

    let a = app.clone();
    s.on_adc_add(move || edit(&a, |t| t.adc_channels.push(AdcChannelForm::new())));

    let a = app.clone();
    s.on_adc_remove(move |ch| {
        edit(&a, |t| {
            if (ch as usize) < t.adc_channels.len() {
                t.adc_channels.remove(ch as usize);
            }
        })
    });

    let a = app.clone();
    s.on_adc_edited(move |ch, field, text| {
        if let Some(c) = a
            .configuration_tab
            .borrow_mut()
            .adc_channels
            .get_mut(ch as usize)
        {
            let text = text.to_string();
            match field.as_str() {
                "name" => c.name = text,
                "channel" => c.channel = text,
                "scale" => c.scale = text,
                "offset" => c.offset = text,
                _ => {}
            }
        }
    });
}
