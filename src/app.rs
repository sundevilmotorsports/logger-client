use slint::{ComponentHandle, JoinHandle, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crate::device::{self, DeviceState};
use crate::log_parse;
use crate::tabs::{self, ConfigurationTab, ConsoleTab, DevicesTab, LogsTab};
use crate::ui::{AppState, AppWindow, Toast};

const TOAST_LIFETIME: Duration = Duration::from_secs(4);

/// Tab order matches the `TabBar` in `ui/app.slint`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Devices,
    Logs,
    Console,
    Configuration,
    Info,
}

impl Tab {
    const ALL: [Tab; 6] = [
        Tab::Home,
        Tab::Devices,
        Tab::Logs,
        Tab::Console,
        Tab::Configuration,
        Tab::Info,
    ];
}

#[derive(Clone, Copy)]
pub enum ToastKind {
    Success,
    Error,
}

pub struct App {
    ui: slint::Weak<AppWindow>,
    pub(crate) state: RefCell<DeviceState>,
    // Kept alive so the device thread's command channel stays open.
    _req_tx: device::RequestTx,
    pub(crate) log_tx: device::LogRequestTx,
    selected_tab: Cell<Tab>,
    pub(crate) devices_tab: RefCell<DevicesTab>,
    pub(crate) logs_tab: RefCell<LogsTab>,
    pub(crate) console_tab: RefCell<ConsoleTab>,
    pub(crate) configuration_tab: RefCell<ConfigurationTab>,
    logs_poll: RefCell<Option<JoinHandle<()>>>,
    devices_poll: RefCell<Option<JoinHandle<()>>>,
    toasts: RefCell<Vec<(u64, String, ToastKind)>>,
    next_toast_id: Cell<u64>,
}

impl App {
    pub fn new(ui: &AppWindow) -> Rc<Self> {
        let (tx, mut rx) = tokio::sync::watch::channel(DeviceState::default());
        let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel();
        let (log_tx, log_rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || device::poll(tx, req_rx, log_rx));

        let app = Rc::new(Self {
            ui: ui.as_weak(),
            state: RefCell::new(DeviceState::default()),
            _req_tx: req_tx,
            log_tx,
            selected_tab: Cell::new(Tab::Home),
            devices_tab: RefCell::default(),
            logs_tab: RefCell::default(),
            console_tab: RefCell::default(),
            configuration_tab: RefCell::default(),
            logs_poll: RefCell::new(None),
            devices_poll: RefCell::new(None),
            toasts: RefCell::new(Vec::new()),
            next_toast_id: Cell::new(0),
        });

        app.wire(ui);
        tabs::devices::wire(&app, ui);
        tabs::logs::wire(&app, ui);
        tabs::console::wire(&app, ui);
        tabs::configuration::wire(&app, ui);

        let this = app.clone();
        slint::spawn_local(async move {
            while rx.changed().await.is_ok() {
                let state = rx.borrow_and_update().clone();
                log::debug!("app: received device state, port={:?}", state.port);
                *this.state.borrow_mut() = state;
                this.render_device_state();
            }
        })
        .expect("event loop not running");

        ConsoleTab::start(&app);
        app.render_device_state();
        app.render_console();
        app.render_configuration();
        app
    }

    fn wire(self: &Rc<Self>, ui: &AppWindow) {
        let state = ui.global::<AppState>();

        let app = self.clone();
        state.on_tab_selected(move |ix| {
            if let Some(tab) = Tab::ALL.get(ix as usize) {
                app.select_tab(*tab);
            }
        });

        let app = self.clone();
        state.on_restart(move || {
            let app = app.clone();
            spawn(async move { run_command_toast(&app, device::Command::Reboot, "restart").await });
        });

        let app = self.clone();
        state.on_parse_log_file(move || {
            let app = app.clone();
            spawn(async move { parse_log_file_flow(&app).await });
        });

        let app = self.clone();
        state.on_parse_log_folder(move || {
            let app = app.clone();
            spawn(async move { parse_log_folder_flow(&app).await });
        });

        state.on_quit(|| {
            slint::quit_event_loop().ok();
        });
    }

    pub(crate) fn with_ui(&self, f: impl FnOnce(&AppWindow)) {
        if let Some(ui) = self.ui.upgrade() {
            f(&ui);
        }
    }

    fn select_tab(self: &Rc<Self>, tab: Tab) {
        self.selected_tab.set(tab);
        self.sync_logs_poll();
        self.sync_devices_poll();
        if tab == Tab::Configuration {
            ConfigurationTab::auto_fetch(self);
        }
    }

    /// Everything that's derived from the polled `DeviceState`.
    fn render_device_state(&self) {
        let state = self.state.borrow();
        self.with_ui(|ui| {
            let app_state = ui.global::<AppState>();
            app_state.set_connected(state.port.is_some());
            app_state.set_title(
                state
                    .port
                    .as_deref()
                    .map(|p| format!("● {p}"))
                    .unwrap_or_else(|| "Logger Client".to_string())
                    .into(),
            );
            tabs::home::render(&state, ui);
            tabs::info::render(&state, ui);
        });
        drop(state);
        self.render_devices();
        self.render_logs();
    }

    pub(crate) fn render_devices(&self) {
        let state = self.state.borrow();
        self.with_ui(|ui| self.devices_tab.borrow_mut().render(&state, ui));
    }

    pub(crate) fn render_logs(&self) {
        let state = self.state.borrow();
        self.with_ui(|ui| self.logs_tab.borrow().render(&state, ui));
    }

    pub(crate) fn render_console(&self) {
        self.with_ui(|ui| self.console_tab.borrow().render(ui));
    }

    pub(crate) fn render_configuration(&self) {
        self.with_ui(|ui| self.configuration_tab.borrow().render(ui));
    }

    fn render_toasts(&self) {
        let toasts: Vec<Toast> = self
            .toasts
            .borrow()
            .iter()
            .map(|(_, message, kind)| Toast {
                message: message.into(),
                error: matches!(kind, ToastKind::Error),
            })
            .collect();
        self.with_ui(|ui| ui.global::<AppState>().set_toasts(model(toasts)));
    }

    pub(crate) fn push_toast(self: &Rc<Self>, message: String, kind: ToastKind) {
        let id = self.next_toast_id.get();
        self.next_toast_id.set(id + 1);
        self.toasts.borrow_mut().push((id, message, kind));
        self.render_toasts();

        let app = self.clone();
        slint::Timer::single_shot(TOAST_LIFETIME, move || {
            app.toasts.borrow_mut().retain(|(tid, _, _)| *tid != id);
            app.render_toasts();
        });
    }

    /// Keeps the Logs tab's entries fresh once a second while it's open;
    /// aborting the task cancels the loop.
    fn sync_logs_poll(self: &Rc<Self>) {
        let mut poll = self.logs_poll.borrow_mut();
        if self.selected_tab.get() != Tab::Logs {
            if let Some(task) = poll.take() {
                task.abort();
            }
            return;
        }
        if poll.is_some() {
            return;
        }
        let app = self.clone();
        *poll = slint::spawn_local(async move {
            loop {
                if let Ok(entries) = device::list_logs(&app.log_tx).await {
                    app.logs_tab.borrow_mut().set_entries(entries);
                    app.render_logs();
                }
                sleep(Duration::from_secs(1)).await;
            }
        })
        .ok();
    }

    /// Same idea as `sync_logs_poll`, for the Devices tab's CAN node list.
    fn sync_devices_poll(self: &Rc<Self>) {
        let mut poll = self.devices_poll.borrow_mut();
        if self.selected_tab.get() != Tab::Devices {
            if let Some(task) = poll.take() {
                task.abort();
            }
            return;
        }
        if poll.is_some() {
            return;
        }
        let app = self.clone();
        *poll = slint::spawn_local(async move {
            loop {
                if let Ok(nodes) = device::can_nodes(&app.log_tx).await {
                    app.devices_tab.borrow_mut().set_nodes(nodes);
                    app.render_devices();
                }
                sleep(Duration::from_secs(1)).await;
            }
        })
        .ok();
    }
}

/// Wraps a `Vec` as a Slint model.
pub(crate) fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}

/// Runs a future on the UI event loop, detached.
pub(crate) fn spawn(fut: impl Future<Output = ()> + 'static) {
    slint::spawn_local(fut).expect("event loop not running");
}

/// An async sleep driven by a Slint timer, for use inside `slint::spawn_local`.
pub(crate) async fn sleep(duration: Duration) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    slint::Timer::single_shot(duration, move || {
        tx.send(()).ok();
    });
    rx.await.ok();
}

/// Runs synchronous, CPU-bound work on its own thread so the UI stays live.
/// The thread starts immediately, not when the returned future is first
/// polled.
pub(crate) fn run_blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = anyhow::Result<T>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        tx.send(f()).ok();
    });
    async move {
        rx.await
            .map_err(|_| anyhow::anyhow!("worker thread panicked"))
    }
}

pub(crate) async fn pick_file(title: &str) -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title(title)
        .pick_file()
        .await
        .map(|f| f.path().to_path_buf())
}

pub(crate) async fn save_file(dir: &Path, suggested_name: &str) -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_directory(dir)
        .set_file_name(suggested_name)
        .save_file()
        .await
        .map(|f| f.path().to_path_buf())
}

/// Sends `cmd` and reports the result as a toast
pub(crate) async fn run_command_toast(app: &Rc<App>, cmd: device::Command, label: &str) {
    let result = device::request(&app.log_tx, cmd).await;
    let (message, kind) = match result {
        Ok(_) => (label.to_string(), ToastKind::Success),
        Err(e) => (format!("{label} failed: {e}"), ToastKind::Error),
    };
    app.push_toast(message, kind);
}

/// `File > Parse Log File...`: pick a `.bin` log already sitting on disk
/// (e.g. pulled straight off the SD card), decode it, prompt for where to
/// save the CSV, and report the result as a toast. Independent of any live
/// device connection.
async fn parse_log_file_flow(app: &Rc<App>) {
    let (message, kind) = match parse_log_file_flow_inner().await {
        Ok(Some(message)) => (message, ToastKind::Success),
        Ok(None) => return, // user cancelled a dialog; nothing to report
        Err(e) => (format!("parse log failed: {e}"), ToastKind::Error),
    };
    app.push_toast(message, kind);
}

async fn parse_log_file_flow_inner() -> anyhow::Result<Option<String>> {
    let Some(src) = pick_file("Parse").await else {
        return Ok(None);
    };

    let raw =
        std::fs::read(&src).map_err(|e| anyhow::anyhow!("couldn't read {}: {e}", src.display()))?;

    let csv_name = format!(
        "{}.csv",
        src.file_stem()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default()
    );
    let save_dir = src
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let Some(dest) = save_file(&save_dir, &csv_name).await else {
        return Ok(None);
    };

    let parsed = run_blocking(move || log_parse::parse_log(&raw, |_, _| {})).await??;

    std::fs::write(&dest, log_parse::to_csv(&parsed))
        .map_err(|e| anyhow::anyhow!("couldn't write {}: {e}", dest.display()))?;

    let ld_dest = dest.with_extension("ld");
    std::fs::write(&ld_dest, crate::motec::build_ld(&parsed))
        .map_err(|e| anyhow::anyhow!("couldn't write {}: {e}", ld_dest.display()))?;

    Ok(Some(format!(
        "parsed {} rows from {}, saved {} and {}",
        parsed.rows.len(),
        src.display(),
        dest.display(),
        ld_dest.display()
    )))
}

/// `File > Parse Log Folder...`: pick a directory, decode every `.bin` log
/// inside it (next to itself as `<name>.csv` and `<name>.ld`), and report a
/// summary as a toast. Independent of any live device connection.
async fn parse_log_folder_flow(app: &Rc<App>) {
    let (message, kind) = match parse_log_folder_flow_inner().await {
        Ok(Some(message)) => (message, ToastKind::Success),
        Ok(None) => return, // user cancelled the dialog; nothing to report
        Err(e) => (format!("parse log folder failed: {e}"), ToastKind::Error),
    };
    app.push_toast(message, kind);
}

async fn parse_log_folder_flow_inner() -> anyhow::Result<Option<String>> {
    let Some(dir) = rfd::AsyncFileDialog::new()
        .set_title("Parse Folder")
        .pick_folder()
        .await
        .map(|f| f.path().to_path_buf())
    else {
        return Ok(None);
    };

    let mut bin_paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| anyhow::anyhow!("couldn't read {}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|ext| ext == "bin"))
        .collect();
    bin_paths.sort();

    if bin_paths.is_empty() {
        return Ok(Some(format!(
            "no .bin log files found in {}",
            dir.display()
        )));
    }

    let (ok_count, total, errors) = run_blocking(move || {
        let mut ok_count = 0usize;
        let mut errors: Vec<String> = Vec::new();
        for src in &bin_paths {
            match parse_one_log_to_csv_and_ld(src) {
                Ok(()) => ok_count += 1,
                Err(e) => errors.push(format!(
                    "{}: {e}",
                    src.file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or_default()
                )),
            }
        }
        (ok_count, bin_paths.len(), errors)
    })
    .await?;

    let summary = if errors.is_empty() {
        format!("parsed {ok_count} log file(s) in {}", dir.display())
    } else {
        format!(
            "parsed {ok_count}/{total} log file(s) in {} ({} failed: {})",
            dir.display(),
            errors.len(),
            errors.join("; ")
        )
    };

    Ok(Some(summary))
}

/// Decodes a single `.bin` log at `src` and writes `<stem>.csv` and
/// `<stem>.ld` alongside it. Runs the (synchronous, CPU-bound) parser on the
/// calling thread -- callers batching many files should already be off the
/// UI thread.
fn parse_one_log_to_csv_and_ld(src: &Path) -> anyhow::Result<()> {
    let raw =
        std::fs::read(src).map_err(|e| anyhow::anyhow!("couldn't read {}: {e}", src.display()))?;
    let parsed = log_parse::parse_log(&raw, |_, _| {})?;

    let csv_dest = src.with_extension("csv");
    std::fs::write(&csv_dest, log_parse::to_csv(&parsed))
        .map_err(|e| anyhow::anyhow!("couldn't write {}: {e}", csv_dest.display()))?;

    let ld_dest = src.with_extension("ld");
    std::fs::write(&ld_dest, crate::motec::build_ld(&parsed))
        .map_err(|e| anyhow::anyhow!("couldn't write {}: {e}", ld_dest.display()))?;

    Ok(())
}
