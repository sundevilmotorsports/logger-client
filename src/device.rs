use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Uptime,
}

#[derive(Debug, Deserialize)]
pub struct StatusResponse {
    pub uptime_seconds: u64,
}

pub fn find_port() -> Option<String> {
    serialport::available_ports().ok()?.into_iter().find_map(|p| match p.port_type {
        serialport::SerialPortType::UsbPort(_) => Some(p.port_name),
        _ => None,
    })
}

pub struct Connection {
    port: Box<dyn serialport::SerialPort>,
}

impl Connection {
    pub fn open(port_name: &str) -> anyhow::Result<Self> {
        let mut port = serialport::new(port_name, 115200)
            .timeout(Duration::from_millis(1000))
            .open()?;
        // Disable DTR/RTS to avoid triggering ESP32 auto-reset on port open
        let _ = port.write_data_terminal_ready(false);
        let _ = port.write_request_to_send(false);
        port.clear(serialport::ClearBuffer::Input).ok();
        Ok(Self { port })
    }

    pub fn request<R>(&mut self, cmd: &Command) -> anyhow::Result<R>
    where
        R: for<'de> Deserialize<'de>,
    {
        let payload = serde_json::to_string(cmd)? + "\n";
        self.port.write_all(payload.as_bytes())?;
        self.port.flush()?;

        let mut buf = Vec::new();
        BufReader::new(&mut *self.port).read_until(b'\n', &mut buf)?;

        let raw = String::from_utf8_lossy(&buf);
        let raw = raw.trim();
        log::debug!("device → {raw}");

        let v: serde_json::Value = serde_json::from_str(raw)
            .map_err(|e| anyhow!("bad JSON ({e}): {raw}"))?;

        // Firmware envelope: {"ok": true, "data": {...}} | {"ok": false, "error": "..."}
        match v.get("ok").and_then(|b| b.as_bool()) {
            Some(true) => {
                let data = v.get("data").cloned().unwrap_or(serde_json::Value::Null);
                Ok(serde_json::from_value(data)?)
            }
            Some(false) => {
                let msg = v["error"].as_str().unwrap_or("unknown error");
                Err(anyhow!("device error: {msg}"))
            }
            None => Ok(serde_json::from_value(v)?), // bare JSON fallback
        }
    }
}

/// Runs a blocking serial loop, sending uptime results through `tx`.
/// Dropping `tx` will cause this function to return.
pub fn serial_loop(port: String, tx: tokio::sync::mpsc::Sender<Option<u64>>) {
    let mut conn = match Connection::open(&port) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("open {port}: {e}");
            return;
        }
    };

    loop {
        let uptime = conn
            .request::<StatusResponse>(&Command::Uptime)
            .map_err(|e| log::warn!("request: {e}"))
            .ok()
            .map(|r| r.uptime_seconds);

        if tx.blocking_send(uptime).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
