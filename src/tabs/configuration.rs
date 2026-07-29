use gpui::{
    AnyElement, AsyncApp, Context, Hsla, IntoElement, PathPromptOptions, WeakEntity, div,
    prelude::*, px,
};
use std::path::PathBuf;

use crate::device::{self, Command, Response};
use crate::root_view::RootView;
use crate::theme;
use crate::toast::ToastKind;

#[derive(Default)]
pub struct ConfigurationTab {
    status: Status,
    raw: Option<serde_json::Value>,
}

#[derive(Default)]
enum Status {
    #[default]
    Idle,
    Loading,
    Error(String),
}

impl ConfigurationTab {
    pub fn render(&self, log_tx: &device::LogRequestTx, cx: &mut Context<RootView>) -> AnyElement {
        let busy = matches!(self.status, Status::Loading);
        let (status_text, status_color) = match &self.status {
            Status::Idle if self.raw.is_none() => ("not loaded".to_string(), theme::muted()),
            Status::Idle => ("loaded from device".to_string(), theme::green()),
            Status::Loading => ("working...".to_string(), theme::amber()),
            Status::Error(e) => (format!("error: {e}"), theme::red()),
        };

        let body = match &self.raw {
            Some(v) => serde_json::to_string_pretty(v).unwrap_or_else(|e| format!("{v:#?} ({e})")),
            None => "No configuration loaded yet. \"refresh\" fetches it from the device; \
                     \"load file...\" sends one from disk."
                .to_string(),
        };

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
                    .text_color(theme::fg())
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
                if busy {
                    return;
                }
                this.configuration_tab.status = Status::Loading;
                cx.notify();
                let log_tx = refresh_tx.clone();
                cx.spawn(async move |weak, cx| refresh(weak, cx, log_tx).await)
                    .detach();
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

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(refresh_btn)
            .child(load_btn)
            .child(save_btn)
            .child(div().text_color(status_color).child(status_text))
            .into_any_element()
    }
}

async fn refresh(weak: WeakEntity<RootView>, cx: &mut AsyncApp, log_tx: device::LogRequestTx) {
    let result = device::request(&log_tx, Command::GetConfig).await;
    weak.update(cx, |view, cx| {
        match result {
            Ok(Response::Config(v)) => {
                view.configuration_tab.raw = Some(v);
                view.configuration_tab.status = Status::Idle;
            }
            Ok(other) => {
                view.configuration_tab.status =
                    Status::Error(format!("unexpected response: {other:?}"));
            }
            Err(e) => view.configuration_tab.status = Status::Error(e.to_string()),
        }
        cx.notify();
    })
    .ok();
}

async fn load_from_file(
    weak: WeakEntity<RootView>,
    cx: &mut AsyncApp,
    log_tx: device::LogRequestTx,
) {
    let result = load_from_file_inner(cx, &log_tx).await;
    weak.update(cx, |view, cx| match result {
        Ok(Some(v)) => {
            view.configuration_tab.raw = Some(v);
            view.configuration_tab.status = Status::Idle;
            view.push_toast(cx, "configuration loaded".to_string(), ToastKind::Success);
        }
        Ok(None) => {
            view.configuration_tab.status = Status::Idle; // dialog cancelled
            cx.notify();
        }
        Err(e) => {
            view.configuration_tab.status = Status::Error(e.to_string());
            view.push_toast(
                cx,
                format!("load configuration failed: {e}"),
                ToastKind::Error,
            );
        }
    })
    .ok();
}

async fn load_from_file_inner(
    cx: &mut AsyncApp,
    log_tx: &device::LogRequestTx,
) -> anyhow::Result<Option<serde_json::Value>> {
    let Some(path) = prompt_open(cx).await? else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;

    match device::request(
        log_tx,
        Command::SetConfig {
            args: value.clone(),
        },
    )
    .await?
    {
        Response::SetConfigOk => Ok(Some(value)),
        other => anyhow::bail!("unexpected response to set_config: {other:?}"),
    }
}

async fn prompt_open(cx: &mut AsyncApp) -> anyhow::Result<Option<PathBuf>> {
    let rx = cx.update(|app| {
        app.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Load Configuration".into()),
        })
    })?;
    let paths = rx
        .await
        .map_err(|_| anyhow::anyhow!("file dialog closed unexpectedly"))??;
    Ok(paths.and_then(|p| p.into_iter().next()))
}

async fn save_to_file(weak: WeakEntity<RootView>, cx: &mut AsyncApp, log_tx: device::LogRequestTx) {
    let result = save_to_file_inner(cx, &log_tx).await;
    weak.update(cx, |view, cx| {
        view.configuration_tab.status = Status::Idle;
        match result {
            Ok(Some((value, path))) => {
                view.configuration_tab.raw = Some(value);
                view.push_toast(
                    cx,
                    format!("saved to {}", path.display()),
                    ToastKind::Success,
                );
            }
            Ok(None) => cx.notify(), // dialog cancelled
            Err(e) => view.push_toast(
                cx,
                format!("save configuration failed: {e}"),
                ToastKind::Error,
            ),
        }
    })
    .ok();
}

async fn save_to_file_inner(
    cx: &mut AsyncApp,
    log_tx: &device::LogRequestTx,
) -> anyhow::Result<Option<(serde_json::Value, PathBuf)>> {
    let value = match device::request(log_tx, Command::GetConfig).await? {
        Response::Config(v) => v,
        other => anyhow::bail!("unexpected response to get_config: {other:?}"),
    };
    let pretty = serde_json::to_string_pretty(&value)?;

    let dir = std::env::current_dir().unwrap_or_default();
    let rx = cx.update(|app| app.prompt_for_new_path(&dir, Some("config.json")))?;
    let Some(dest) = rx
        .await
        .map_err(|_| anyhow::anyhow!("save dialog closed unexpectedly"))??
    else {
        return Ok(None);
    };
    std::fs::write(&dest, pretty)?;
    Ok(Some((value, dest)))
}
