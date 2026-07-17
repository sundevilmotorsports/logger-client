use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use anyhow::anyhow;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Ping,
    Status,
    GetConfig,
    SetConfig { args: serde_json::Value },
    Frames,
    Uptime,
    Gps,
}

#[derive(Debug)]
pub enum Response {
    Pong,
    Status { config_loaded: bool },
    Config(serde_json::Value),
    SetConfigOk,
    Frames(Vec<serde_json::Value>),
    Uptime { uptime_seconds: u64 },
    Gps(GpsFix),
}

#[derive(Debug, Clone)]
pub struct GpsFix {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
    pub sats: u64,
}

struct Connection {
    writer: Box<dyn serialport::SerialPort>,
    reader: BufReader<Box<dyn serialport::SerialPort>>,
}

impl Connection {
    fn open(port_name: &str) -> anyhow::Result<Self> {
        let mut port = serialport::new(port_name, 115200)
            .timeout(Duration::from_millis(2000))
            .open()?;
        let _ = port.write_data_terminal_ready(false);
        let _ = port.write_request_to_send(false);
        std::thread::sleep(Duration::from_millis(1500));
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
            Command::Status => Response::Status {
                config_loaded: data["config_loaded"].as_bool().unwrap_or(false),
            },
            Command::GetConfig => Response::Config(data),
            Command::SetConfig { .. } => Response::SetConfigOk,
            Command::Frames => Response::Frames(data.as_array().cloned().unwrap_or_default()),
            Command::Uptime => Response::Uptime {
                uptime_seconds: data["uptime_seconds"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("missing uptime_seconds in: {data}"))?,
            },
            Command::Gps => Response::Gps(GpsFix {
                lat: data["lat"]
                    .as_f64()
                    .ok_or_else(|| anyhow!("missing lat in: {data}"))?,
                lon: data["lon"]
                    .as_f64()
                    .ok_or_else(|| anyhow!("missing lon in: {data}"))?,
                alt_m: data["alt_m"].as_f64().unwrap_or(0.0),
                sats: data["sats"].as_u64().unwrap_or(0),
            }),
        })
    }
}

pub type RequestTx = tokio::sync::mpsc::UnboundedSender<Command>;

pub fn find_port() -> Option<String> {
    serialport::available_ports()
        .ok()?
        .into_iter()
        .find_map(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(_) | serialport::SerialPortType::Unknown => {
                Some(p.port_name)
            }
            _ => None,
        })
}

#[derive(Clone, Default)]
pub struct DeviceState {
    pub port: Option<String>,
    pub uptime: Option<u64>,
    pub last_ping: Option<bool>,
    pub gps: Option<GpsFix>,
}

pub fn poll(
    tx: tokio::sync::watch::Sender<DeviceState>,
    mut req_rx: tokio::sync::mpsc::UnboundedReceiver<Command>,
) {
    let mut state = DeviceState::default();
    loop {
        let Some(port_name) = find_port() else {
            if state.port.is_some() {
                state = DeviceState::default();
                tx.send(state.clone()).ok();
            }
            std::thread::sleep(Duration::from_secs(3));
            continue;
        };

        let maybe_conn = (|| {
            let mut conn = Connection::open(&port_name).ok()?;
            matches!(conn.request(Command::Ping), Ok(Response::Pong)).then_some(conn)
        })();

        let Some(mut conn) = maybe_conn else {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };

        state = DeviceState {
            port: Some(port_name),
            ..Default::default()
        };
        tx.send(state.clone()).ok();

        loop {
            while let Ok(cmd) = req_rx.try_recv() {
                match cmd {
                    Command::Ping => {
                        state.last_ping = Some(conn.request(Command::Ping).is_ok());
                        tx.send(state.clone()).ok();
                    }
                    _ => log::warn!("unhandled command: {cmd:?}"),
                }
            }

            match conn.request(Command::Uptime) {
                Ok(Response::Uptime { uptime_seconds }) => {
                    state.uptime = Some(uptime_seconds);
                    state.gps = match conn.request(Command::Gps) {
                        Ok(Response::Gps(fix)) => Some(fix),
                        _ => None,
                    };
                    if tx.send(state.clone()).is_err() {
                        return;
                    }
                }
                other => {
                    log::warn!("uptime: {other:?}");
                    break;
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }

        state = DeviceState::default();
        tx.send(state.clone()).ok();
    }
}
