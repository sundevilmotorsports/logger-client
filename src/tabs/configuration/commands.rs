use std::path::PathBuf;
use std::rc::Rc;

use crate::app::{self, App, ToastKind};
use crate::device::{self, Command, Response};

use super::Status;

pub(super) async fn refresh(app: &Rc<App>) {
    let result = device::request(&app.log_tx, Command::GetConfig).await;
    {
        let mut tab = app.configuration_tab.borrow_mut();
        match result {
            Ok(Response::Config(v)) => tab.load(&v),
            Ok(other) => tab.status = Status::Error(format!("unexpected response: {other:?}")),
            Err(e) => tab.status = Status::Error(e.to_string()),
        }
    }
    app.render_configuration();
}

pub(super) async fn push_to_device(app: &Rc<App>, value: serde_json::Value) {
    let result = device::request(&app.log_tx, Command::SetConfig { args: value }).await;
    app.configuration_tab.borrow_mut().status = Status::Idle;
    app.render_configuration();
    match result {
        Ok(Response::SetConfigOk) => {
            app.push_toast("configuration pushed".to_string(), ToastKind::Success);
        }
        Ok(other) => app.push_toast(format!("unexpected response: {other:?}"), ToastKind::Error),
        Err(e) => app.push_toast(format!("push failed: {e}"), ToastKind::Error),
    }
}

pub(super) async fn load_from_file(app: &Rc<App>) {
    let result = load_from_file_inner(&app.log_tx).await;
    match result {
        Ok(Some(v)) => {
            app.configuration_tab.borrow_mut().load(&v);
            app.render_configuration();
            app.push_toast("configuration loaded".to_string(), ToastKind::Success);
        }
        Ok(None) => {
            // dialog cancelled
            app.configuration_tab.borrow_mut().status = Status::Idle;
            app.render_configuration();
        }
        Err(e) => {
            app.configuration_tab.borrow_mut().status = Status::Error(e.to_string());
            app.render_configuration();
            app.push_toast(format!("load configuration failed: {e}"), ToastKind::Error);
        }
    }
}

async fn load_from_file_inner(
    log_tx: &device::LogRequestTx,
) -> anyhow::Result<Option<serde_json::Value>> {
    let Some(path) = app::pick_file("Load Configuration").await else {
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

pub(super) async fn save_to_file(app: &Rc<App>) {
    let result = save_to_file_inner(&app.log_tx).await;
    app.configuration_tab.borrow_mut().status = Status::Idle;
    match result {
        Ok(Some((value, path))) => {
            app.configuration_tab.borrow_mut().load(&value);
            app.render_configuration();
            app.push_toast(format!("saved to {}", path.display()), ToastKind::Success);
        }
        Ok(None) => app.render_configuration(), // dialog cancelled
        Err(e) => {
            app.render_configuration();
            app.push_toast(format!("save configuration failed: {e}"), ToastKind::Error);
        }
    }
}

async fn save_to_file_inner(
    log_tx: &device::LogRequestTx,
) -> anyhow::Result<Option<(serde_json::Value, PathBuf)>> {
    let value = match device::request(log_tx, Command::GetConfig).await? {
        Response::Config(v) => v,
        other => anyhow::bail!("unexpected response to get_config: {other:?}"),
    };
    let pretty = serde_json::to_string_pretty(&value)?;

    let dir = std::env::current_dir().unwrap_or_default();
    let Some(dest) = app::save_file(&dir, "config.json").await else {
        return Ok(None);
    };
    std::fs::write(&dest, pretty)?;
    Ok(Some((value, dest)))
}
