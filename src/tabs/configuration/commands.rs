use gpui::{AsyncApp, PathPromptOptions, WeakEntity};
use std::path::PathBuf;

use crate::device::{self, Command, Response};
use crate::root_view::RootView;
use crate::toast::ToastKind;

use super::Status;
use super::model::{adc_channels_from_json, can_devices_from_json};

pub(super) async fn refresh(
    weak: WeakEntity<RootView>,
    cx: &mut AsyncApp,
    log_tx: device::LogRequestTx,
) {
    let result = device::request(&log_tx, Command::GetConfig).await;
    weak.update(cx, |view, cx| {
        match result {
            Ok(Response::Config(v)) => {
                view.configuration_tab.can_devices = can_devices_from_json(&v, cx);
                view.configuration_tab.adc_channels = adc_channels_from_json(&v, cx);
                view.configuration_tab.loaded = true;
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

pub(super) async fn push_to_device(
    weak: WeakEntity<RootView>,
    cx: &mut AsyncApp,
    log_tx: device::LogRequestTx,
    value: serde_json::Value,
) {
    let result = device::request(&log_tx, Command::SetConfig { args: value }).await;
    weak.update(cx, |view, cx| {
        view.configuration_tab.status = Status::Idle;
        match result {
            Ok(Response::SetConfigOk) => {
                view.push_toast(cx, "configuration pushed".to_string(), ToastKind::Success);
            }
            Ok(other) => view.push_toast(
                cx,
                format!("unexpected response: {other:?}"),
                ToastKind::Error,
            ),
            Err(e) => view.push_toast(cx, format!("push failed: {e}"), ToastKind::Error),
        }
    })
    .ok();
}

pub(super) async fn load_from_file(
    weak: WeakEntity<RootView>,
    cx: &mut AsyncApp,
    log_tx: device::LogRequestTx,
) {
    let result = load_from_file_inner(cx, &log_tx).await;
    weak.update(cx, |view, cx| match result {
        Ok(Some(v)) => {
            view.configuration_tab.can_devices = can_devices_from_json(&v, cx);
            view.configuration_tab.adc_channels = adc_channels_from_json(&v, cx);
            view.configuration_tab.loaded = true;
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

pub(super) async fn save_to_file(
    weak: WeakEntity<RootView>,
    cx: &mut AsyncApp,
    log_tx: device::LogRequestTx,
) {
    let result = save_to_file_inner(cx, &log_tx).await;
    weak.update(cx, |view, cx| {
        view.configuration_tab.status = Status::Idle;
        match result {
            Ok(Some((value, path))) => {
                view.configuration_tab.can_devices = can_devices_from_json(&value, cx);
                view.configuration_tab.adc_channels = adc_channels_from_json(&value, cx);
                view.configuration_tab.loaded = true;
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
