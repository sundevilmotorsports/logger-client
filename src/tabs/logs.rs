use slint::ComponentHandle;
use std::path::PathBuf;
use std::rc::Rc;

use crate::app::{self, App, model};
use crate::device::{self, DeviceState};
use crate::log_parse;
use crate::motec;
use crate::ui::{AppWindow, LogRow, LogsState, Tone};

#[derive(Default)]
pub struct LogsTab {
    entries: Vec<device::LogEntry>,
    busy: Option<Busy>,
    last_result: Option<Result<String, String>>,
}

struct Busy {
    name: String,
    phase: &'static str,
    done: u64,
    total: u64,
}

impl Busy {
    fn fraction(&self) -> f32 {
        if self.total == 0 {
            1.
        } else {
            (self.done as f32 / self.total as f32).min(1.)
        }
    }
}

pub fn wire(app: &Rc<App>, ui: &AppWindow) {
    let s = ui.global::<LogsState>();

    let a = app.clone();
    s.on_pause(move || {
        let resume = !matches!(a.state.borrow().logging_active, Some(true));
        let a = a.clone();
        app::spawn(async move {
            let label = if resume { "resume" } else { "pause" };
            app::run_command_toast(&a, device::Command::SetLogging { active: resume }, label).await
        });
    });

    let a = app.clone();
    s.on_new_log(move || {
        let a = a.clone();
        app::spawn(
            async move { app::run_command_toast(&a, device::Command::NextLog, "new log").await },
        );
    });

    let a = app.clone();
    s.on_get(move |ix| {
        let name = {
            let mut tab = a.logs_tab.borrow_mut();
            if tab.busy.is_some() {
                return;
            }
            let Some(entry) = tab.entries.get(ix as usize) else {
                return;
            };
            let (name, total) = (entry.name.clone(), entry.size);
            tab.busy = Some(Busy {
                name: name.clone(),
                phase: "downloading",
                done: 0,
                total,
            });
            tab.last_result = None;
            name
        };
        a.render_logs();

        let a = a.clone();
        app::spawn(async move { run_job(&a, name).await });
    });
}

impl LogsTab {
    /// Used by the periodic background poll (see `App::sync_logs_poll`).
    /// Sorted newest first -- names are zero-padded ("0001.bin", "0002.bin",
    /// ...) so a plain string sort already puts them in numeric order.
    pub(crate) fn set_entries(&mut self, mut entries: Vec<device::LogEntry>) {
        entries.sort_by(|a, b| b.name.cmp(&a.name));
        self.entries = entries;
    }

    pub fn render(&self, state: &DeviceState, ui: &AppWindow) {
        let rows: Vec<LogRow> = self
            .entries
            .iter()
            .map(|entry| {
                let busy = self.busy.as_ref().filter(|b| b.name == entry.name);
                LogRow {
                    name: entry.name.clone().into(),
                    size: format_size(entry.size).into(),
                    current: state.current_log.as_deref() == Some(entry.name.as_str())
                        && state.logging_active == Some(true),
                    busy: busy.is_some(),
                    progress: busy.map_or(0., Busy::fraction),
                    phase: busy
                        .map(|b| format!("{} {:.0}%", b.phase, b.fraction() * 100.))
                        .unwrap_or_default()
                        .into(),
                }
            })
            .collect();

        let s = ui.global::<LogsState>();
        s.set_rows(model(rows));
        s.set_status_text(if self.entries.is_empty() {
            "".into()
        } else {
            format!("{} log(s)", self.entries.len()).into()
        });
        s.set_status_tone(Tone::Muted);

        let (pause_label, pause_enabled) = match state.logging_active {
            Some(true) => ("pause", true),
            Some(false) => ("resume", true),
            None => ("pause", false),
        };
        s.set_pause_label(pause_label.into());
        s.set_pause_enabled(pause_enabled);

        let (text, tone) = match &self.last_result {
            Some(Ok(m)) => (m.as_str(), Tone::Green),
            Some(Err(m)) => (m.as_str(), Tone::Red),
            None => ("", Tone::Muted),
        };
        s.set_result_text(text.into());
        s.set_result_tone(tone);
    }
}

fn set_progress(app: &App, done: u64) {
    if let Some(busy) = &mut app.logs_tab.borrow_mut().busy {
        busy.done = done;
    }
    app.render_logs();
}

async fn run_job(app: &Rc<App>, name: String) {
    let result = run_job_inner(app, &name).await;
    {
        let mut tab = app.logs_tab.borrow_mut();
        tab.busy = None;
        tab.last_result = Some(result.map_err(|e| e.to_string()));
    }
    app.render_logs();
}

async fn run_job_inner(app: &Rc<App>, name: &str) -> anyhow::Result<String> {
    let csv_name = format!("{}.csv", name.trim_end_matches(".bin"));
    let Some(dest) = app::save_file(&std::env::temp_dir(), &csv_name).await else {
        return Ok("cancelled".to_string());
    };

    let raw = device::download_log(&app.log_tx, name, |done| set_progress(app, done)).await?;
    let bin_dir = std::env::temp_dir().join("sdm-logger");
    std::fs::create_dir_all(&bin_dir)?;
    let bin_path: PathBuf = bin_dir.join(name);
    std::fs::write(&bin_path, &raw)?;

    if let Some(busy) = &mut app.logs_tab.borrow_mut().busy {
        busy.phase = "parsing";
    }
    set_progress(app, 0);
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<u64>();
    let parse = app::run_blocking(move || {
        log_parse::parse_log(&raw, move |done, _total| {
            progress_tx.send(done as u64).ok();
        })
    });
    // The sender drops when parsing finishes, which ends this loop.
    while let Some(done) = progress_rx.recv().await {
        set_progress(app, done);
    }
    let parsed = parse.await??;

    std::fs::write(&dest, log_parse::to_csv(&parsed))?;

    let ld_dest = dest.with_extension("ld");
    std::fs::write(&ld_dest, motec::build_ld(&parsed))?;

    Ok(format!(
        "parsed {} rows, saved {}, {} and {}",
        parsed.rows.len(),
        bin_path.display(),
        dest.display(),
        ld_dest.display()
    ))
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
