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
}

#[derive(Debug)]
pub enum Response {
    Pong,
    Status { config_loaded: bool },
    Config(serde_json::Value),
    SetConfigOk,
    Frames(Vec<serde_json::Value>),
    Uptime { uptime_seconds: u64 },
}

pub struct Connection {
    port: Box<dyn serialport::SerialPort>,
}

impl Connection {
    pub fn open(port_name: &str) -> anyhow::Result<Self> {
        let mut port = serialport::new(port_name, 115200)
            .timeout(Duration::from_millis(1000))
            .open()?;
        let _ = port.write_data_terminal_ready(false);
        let _ = port.write_request_to_send(false);
        port.clear(serialport::ClearBuffer::Input).ok();
        Ok(Self { port })
    }

    pub fn request(&mut self, cmd: Command) -> anyhow::Result<Response> {
        let json = serde_json::to_string(&cmd)? + "\n";
        self.port.write_all(json.as_bytes())?;
        self.port.flush()?;

        let mut buf = Vec::new();
        BufReader::new(&mut *self.port).read_until(b'\n', &mut buf)?;

        let raw = String::from_utf8_lossy(&buf);
        let raw = raw.trim();
        log::debug!("device → {raw}");

        let envelope: serde_json::Value = serde_json::from_str(raw)
            .map_err(|e| anyhow!("bad JSON ({e}): {raw}"))?;

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
        })
    }
}

pub fn find_port() -> Option<String> {
    serialport::available_ports().ok()?.into_iter().find_map(|p| match p.port_type {
        serialport::SerialPortType::UsbPort(_) => Some(p.port_name),
        _ => None,
    })
}
