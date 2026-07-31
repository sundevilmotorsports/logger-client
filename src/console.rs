use serialport::{SerialPort, SerialPortType};
use std::collections::VecDeque;
use std::io::BufRead;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_LINES: usize = 5000;
const ESPRESSIF_VID: u16 = 0x303a;
const JTAG_SERIAL_PID: u16 = 0x1001;

pub fn find_port() -> Option<String> {
    serialport::available_ports()
        .ok()?
        .into_iter()
        .find_map(|p| match p.port_type {
            SerialPortType::UsbPort(info)
                if info.vid == ESPRESSIF_VID && info.pid == JTAG_SERIAL_PID =>
            {
                Some(p.port_name)
            }
            _ => None,
        })
}

pub fn open(port_name: &str) -> anyhow::Result<Box<dyn SerialPort>> {
    Ok(serialport::new(port_name, 115200)
        .timeout(Duration::from_millis(500))
        .open()?)
}

/// Strips ANSI CSI escape sequences (`\x1b[...<final byte>`)
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            for c2 in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&c2) {
                    break; // final byte of the escape sequence
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[derive(Default)]
pub struct SharedLines {
    lines: Mutex<VecDeque<String>>,
    len: AtomicUsize,
}

impl SharedLines {
    fn push(&self, line: String) {
        let mut lines = self.lines.lock().unwrap();
        lines.push_back(line);
        if lines.len() > MAX_LINES {
            lines.pop_front();
        }
        self.len.store(lines.len(), Ordering::Relaxed);
    }

    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> VecDeque<String> {
        self.lines.lock().unwrap().clone()
    }
}

pub type Lines = Arc<SharedLines>;

pub fn spawn_reader(port: Box<dyn SerialPort>, lines: Lines) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(port);
        loop {
            let mut buf = String::new();
            match reader.read_line(&mut buf) {
                Ok(0) => return, // port closed
                Ok(_) => {
                    let line = strip_ansi(buf.trim_end());
                    if !line.is_empty() {
                        lines.push(line);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(e) => {
                    log::warn!("console: read error: {e}");
                    return;
                }
            }
        }
    })
}
