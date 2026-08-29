use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Ping,
    Version,
    Status,
    GetConfig,
    SetConfig { args: serde_json::Value },
    Frames,
    CanNodes,
    Uptime,
    Gps,
    Imu,
    Resources,
    ListLogs,
    LogChunk { name: String, offset: u64 },
    LogStatus,
    SetLogging { active: bool },
    NextLog,
    Reboot,
    OtaUpload { offset: u64, data: String },
    OtaFlash { node: u8, size: u32, crc: u32 },
    OtaStatus,
}

#[derive(Debug)]
pub enum Response {
    Pong,
    Version { version: String },
    Status {
        config_loaded: bool,
        subsystems: DeviceStatusFlags,
    },
    Config(serde_json::Value),
    SetConfigOk,
    Frames(Vec<serde_json::Value>),
    CanNodes(Vec<CanNode>),
    Uptime { uptime_seconds: u64 },
    Gps(GpsFix),
    Imu(ImuReading),
    Resources(Resources),
    Logs(Vec<LogEntry>),
    LogChunk { data: Vec<u8>, eof: bool },
    LogStatus { active: bool, current: String },
    SetLoggingOk,
    NextLogOk,
    RebootOk,
    OtaUpload { committed: u32 },
    OtaFlashOk,
    OtaStatus(OtaStatus),
}

#[derive(Debug, Clone, Deserialize)]
pub struct OtaStatus {
    pub active: bool,
    pub sent: u32,
    pub total: u32,
    /// `None` while running; `Some(0)` = success, else a failure code:
    /// `1..=6` `can_ota_result`, `0xFE` node silent, `0xFD` upload starved,
    /// `0xF2` logger flash write, `0xF3` logger verify.
    pub result: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImuReading {
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
    pub temp_c: f32,
    pub mag_ut: [f32; 3],
}

/// Extra fields the firmware sends (`quality`, `utc`) are simply ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct GpsFix {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
    pub sats: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Resources {
    pub heap_free: u32,
    pub heap_min_free: u32,
    pub cpu_load: [f32; 2],
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogEntry {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CanNode {
    pub node: u8,
    #[serde(rename = "type")]
    pub device_type: u8,
    /// Millis since this node's last heartbeat
    pub age_ms: i64,
}

/// Per-subsystem init/health flags reported by the firmware's `Status` command.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct DeviceStatusFlags {
    pub adc: bool,
    pub can: bool,
    pub gnss: bool,
    pub imu: bool,
    pub logging: bool,
    pub sd: bool,
    pub usb_hs: bool,
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn hex_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(anyhow!("odd-length hex string"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| anyhow!("bad hex: {e}")))
        .collect()
}

struct Connection {
    writer: Box<dyn serialport::SerialPort>,
    reader: BufReader<Box<dyn serialport::SerialPort>>,
}

impl Connection {
    fn open(port_name: &str) -> anyhow::Result<Self> {
        let mut port = serialport::new(port_name, 115200)
            .timeout(Duration::from_millis(12_000))
            .open()?;
        port.clear(serialport::ClearBuffer::Input).ok();
        let writer = port.try_clone()?;
        Ok(Self {
            writer,
            reader: BufReader::new(port),
        })
    }

    fn request(&mut self, cmd: Command) -> anyhow::Result<Response> {
        let json = serde_json::to_string(&cmd)? + "\n";
        self.writer.write_all(json.as_bytes())?;
        self.writer.flush()?;

        // Device may emit non-JSON debug lines; skip until we get a JSON response.
        let envelope = loop {
            let mut buf = Vec::new();
            self.reader.read_until(b'\n', &mut buf)?;
            let raw = String::from_utf8_lossy(&buf);
            let raw = raw.trim();
            log::debug!("device → {raw}");
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
                break v;
            }
        };

        if envelope["ok"].as_bool() != Some(true) {
            let msg = envelope["error"].as_str().unwrap_or("unknown error");
            return Err(anyhow!("device error: {msg}"));
        }
        let data = envelope["data"].clone();

        Ok(match cmd {
            Command::Ping => Response::Pong,
            Command::Version => Response::Version {
                version: data["version"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing version in: {data}"))?
                    .to_string(),
            },
            Command::Status => Response::Status {
                config_loaded: data["config_loaded"].as_bool().unwrap_or(false),
                subsystems: serde_json::from_value(data["subsystems"].clone()).unwrap_or_default(),
            },
            Command::GetConfig => Response::Config(data),
            Command::SetConfig { .. } => Response::SetConfigOk,
            Command::Frames => Response::Frames(data.as_array().cloned().unwrap_or_default()),
            Command::CanNodes => Response::CanNodes(
                serde_json::from_value(data.clone())
                    .map_err(|e| anyhow!("bad can_nodes data ({e}): {data}"))?,
            ),
            Command::Uptime => Response::Uptime {
                uptime_seconds: data["uptime_seconds"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("missing uptime_seconds in: {data}"))?,
            },
            Command::Gps => Response::Gps(
                serde_json::from_value(data.clone())
                    .map_err(|e| anyhow!("bad gps data ({e}): {data}"))?,
            ),
            Command::Imu => Response::Imu(
                serde_json::from_value(data.clone())
                    .map_err(|e| anyhow!("bad imu data ({e}): {data}"))?,
            ),
            Command::Resources => Response::Resources(
                serde_json::from_value(data.clone())
                    .map_err(|e| anyhow!("bad resources data ({e}): {data}"))?,
            ),
            Command::ListLogs => Response::Logs(
                serde_json::from_value(data.clone())
                    .map_err(|e| anyhow!("bad log list ({e}): {data}"))?,
            ),
            Command::LogChunk { .. } => Response::LogChunk {
                data: hex_decode(data["data"].as_str().unwrap_or_default())?, // binary, hex-encoded
                eof: data["eof"].as_bool().unwrap_or(true),
            },
            Command::LogStatus => Response::LogStatus {
                active: data["active"].as_bool().unwrap_or(false),
                current: data["current"].as_str().unwrap_or_default().to_string(),
            },
            Command::SetLogging { .. } => Response::SetLoggingOk,
            Command::NextLog => Response::NextLogOk,
            Command::Reboot => Response::RebootOk,
            Command::OtaUpload { .. } => Response::OtaUpload {
                committed: data["committed"].as_u64().unwrap_or(0) as u32,
            },
            Command::OtaFlash { .. } => Response::OtaFlashOk,
            Command::OtaStatus => Response::OtaStatus(
                serde_json::from_value(data.clone())
                    .map_err(|e| anyhow!("bad ota_status ({e}): {data}"))?,
            ),
        })
    }
}

pub type RequestTx = tokio::sync::mpsc::UnboundedSender<Command>;

/// A command paired with a place to send its specific reply, unlike
/// `RequestTx` which is fire-and-forget (its results surface later via
/// `DeviceState`). Used for on-demand requests like log listing/download
/// that need their own response instead of the broadcast poll state.
pub struct LogRequest {
    pub cmd: Command,
    pub reply: tokio::sync::oneshot::Sender<anyhow::Result<Response>>,
}

pub type LogRequestTx = tokio::sync::mpsc::UnboundedSender<LogRequest>;

pub async fn request(log_tx: &LogRequestTx, cmd: Command) -> anyhow::Result<Response> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    log_tx
        .send(LogRequest { cmd, reply })
        .map_err(|_| anyhow!("device thread gone"))?;
    rx.await
        .map_err(|_| anyhow!("device thread dropped the reply"))?
}

pub async fn list_logs(log_tx: &LogRequestTx) -> anyhow::Result<Vec<LogEntry>> {
    match request(log_tx, Command::ListLogs).await? {
        Response::Logs(entries) => Ok(entries),
        other => Err(anyhow!("unexpected response to list_logs: {other:?}")),
    }
}

pub async fn can_nodes(log_tx: &LogRequestTx) -> anyhow::Result<Vec<CanNode>> {
    match request(log_tx, Command::CanNodes).await? {
        Response::CanNodes(nodes) => Ok(nodes),
        other => Err(anyhow!("unexpected response to can_nodes: {other:?}")),
    }
}

const CHUNK_RETRIES: u32 = 3;

/// Downloads `name` in full, requesting successive chunks until the device
/// reports end-of-file. Each chunk gets a few retries — a multi-hundred-chunk
/// download shouldn't abort over one transient serial hiccup.
pub async fn download_log(
    log_tx: &LogRequestTx,
    name: &str,
    mut on_progress: impl FnMut(u64),
) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let offset = bytes.len() as u64;
        let cmd = Command::LogChunk {
            name: name.to_string(),
            offset,
        };

        let mut attempt = 0;
        let response = loop {
            match request(log_tx, cmd.clone()).await {
                Ok(response) => break response,
                Err(e) if attempt < CHUNK_RETRIES => {
                    attempt += 1;
                    log::warn!("log_chunk offset={offset} failed ({e}), retry {attempt}");
                }
                Err(e) => return Err(e),
            }
        };

        match response {
            Response::LogChunk { data, eof } => {
                bytes.extend_from_slice(&data);
                on_progress(bytes.len() as u64);
                if eof {
                    return Ok(bytes);
                }
            }
            other => return Err(anyhow!("unexpected response to log_chunk: {other:?}")),
        }
    }
}

#[cfg(target_os = "linux")]
fn usb_speed_mbps(port_name: &str) -> Option<String> {
    let tty_name = port_name.rsplit('/').next()?;
    std::fs::read_to_string(format!("/sys/class/tty/{tty_name}/device/../speed"))
        .ok()
        .map(|s| s.trim().to_string())
}

#[cfg(not(target_os = "linux"))]
fn usb_speed_mbps(_port_name: &str) -> Option<String> {
    None
}

const ESPRESSIF_VID: u16 = 0x303a;
const ROM_DOWNLOAD_PID: u16 = 0x1001;

pub fn find_port() -> Option<String> {
    serialport::available_ports()
        .ok()?
        .into_iter()
        .find_map(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(info)
                if info.vid == ESPRESSIF_VID && info.pid != ROM_DOWNLOAD_PID =>
            {
                Some(p.port_name)
            }
            _ => None,
        })
}

#[derive(Clone, Default)]
pub struct DeviceState {
    pub port: Option<String>,
    pub firmware_version: Option<String>,
    pub uptime: Option<u64>,
    pub last_ping: Option<bool>,
    pub gps: Option<GpsFix>,
    pub imu: Option<ImuReading>,
    pub logging_active: Option<bool>,
    pub current_log: Option<String>,
    pub status: Option<DeviceStatusFlags>,
    pub resources: Option<Resources>,
}

pub fn poll(
    tx: tokio::sync::watch::Sender<DeviceState>,
    mut req_rx: tokio::sync::mpsc::UnboundedReceiver<Command>,
    mut log_rx: tokio::sync::mpsc::UnboundedReceiver<LogRequest>,
) {
    let mut state = DeviceState::default();
    loop {
        let Some(port_name) = find_port() else {
            log::debug!("scanning: no candidate port");
            if state.port.is_some() {
                state = DeviceState::default();
                tx.send(state.clone()).ok();
            }
            std::thread::sleep(Duration::from_secs(3));
            continue;
        };
        log::debug!("scanning: candidate port {port_name}");

        let maybe_conn = (|| {
            let mut conn = Connection::open(&port_name).ok()?;
            matches!(conn.request(Command::Ping), Ok(Response::Pong)).then_some(conn)
        })();

        let Some(mut conn) = maybe_conn else {
            log::debug!("scanning: {port_name} did not respond to ping, retrying");
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };

        log::info!("connected to {port_name}");
        match usb_speed_mbps(&port_name) {
            Some(speed) => log::debug!("USB link speed: {speed} Mbit/s"),
            None => log::debug!("USB link speed: unknown"),
        }
        state = DeviceState {
            port: Some(port_name),
            firmware_version: match conn.request(Command::Version) {
                Ok(Response::Version { version }) => Some(version),
                _ => None,
            },
            ..Default::default()
        };
        tx.send(state.clone()).ok();

        // Due immediately so the first loop iteration fetches uptime/gps/log
        // status right away, same as before this was rate-limited.
        let mut last_heartbeat = Instant::now() - Duration::from_secs(1);

        loop {
            while let Ok(cmd) = req_rx.try_recv() {
                match cmd {
                    Command::Ping => {
                        state.last_ping = Some(conn.request(Command::Ping).is_ok());
                        tx.send(state.clone()).ok();
                    }
                    Command::SetLogging { .. } | Command::NextLog | Command::Reboot => {
                        if let Err(e) = conn.request(cmd) {
                            log::warn!("device control command failed: {e}");
                        }
                    }
                    _ => log::warn!("unhandled command: {cmd:?}"),
                }
            }

            // Chunk requests are drained every loop tick (below), not gated
            // behind the once-a-second heartbeat, so a multi-chunk download
            // isn't throttled to ~1 chunk/sec.
            while let Ok(req) = log_rx.try_recv() {
                req.reply.send(conn.request(req.cmd)).ok();
            }

            if last_heartbeat.elapsed() >= Duration::from_secs(1) {
                last_heartbeat = Instant::now();
                match conn.request(Command::Uptime) {
                    Ok(Response::Uptime { uptime_seconds }) => {
                        state.uptime = Some(uptime_seconds);
                        state.gps = match conn.request(Command::Gps) {
                            Ok(Response::Gps(fix)) => Some(fix),
                            _ => None,
                        };
                        state.imu = match conn.request(Command::Imu) {
                            Ok(Response::Imu(reading)) => Some(reading),
                            _ => None,
                        };
                        state.status = match conn.request(Command::Status) {
                            Ok(Response::Status { subsystems, .. }) => Some(subsystems),
                            _ => None,
                        };
                        state.resources = match conn.request(Command::Resources) {
                            Ok(Response::Resources(r)) => Some(r),
                            _ => None,
                        };
                        match conn.request(Command::LogStatus) {
                            Ok(Response::LogStatus { active, current }) => {
                                state.logging_active = Some(active);
                                state.current_log = Some(current);
                            }
                            _ => {
                                state.logging_active = None;
                                state.current_log = None;
                            }
                        }
                        if tx.send(state.clone()).is_err() {
                            return;
                        }
                    }
                    other => {
                        log::warn!("uptime: {other:?}");
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        state = DeviceState::default();
        tx.send(state.clone()).ok();
    }
}
