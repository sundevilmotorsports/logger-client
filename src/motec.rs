use crate::log_parse::ParsedLog;

const LOG_HEADER_SIZE: usize = 1762;
const EVENT_SIZE: usize = 1154;
const CHANNEL_HEADER_SIZE: usize = 124;

/// Channels the CSV keeps but that aren't meaningful in Motec
const EXCLUDED_CHANNELS: &[&str] = &["reset_reason", "RESET_REASON"];

/// A resolved MoTeC channel: display/short names, units, and the samples
struct Channel {
    name: String,
    short_name: String,
    units: String,
    freq_hz: u16,
    samples: Vec<f32>,
}

/// Builds a `.ld` file from an already-parsed log (see [`crate::log_parse`])
pub fn build_ld(parsed: &ParsedLog) -> Vec<u8> {
    let freq_hz = infer_freq_hz(parsed).unwrap_or(20); // 20 Hz matches logging.rs's LOG_HZ

    let ts_index = parsed
        .columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case("timestamp"));

    let channels: Vec<Channel> = parsed
        .columns
        .iter()
        .enumerate()
        .filter(|(idx, name)| Some(*idx) != ts_index && !is_excluded(name))
        .map(|(idx, name)| {
            let meta = lookup(name);
            let samples = parsed
                .rows
                .iter()
                .map(|row| {
                    let raw: f64 = row.get(idx).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    (meta.convert)(raw) as f32
                })
                .collect();
            Channel {
                name: meta.display.to_string(),
                short_name: meta.short.to_string(),
                units: meta.units.to_string(),
                freq_hz,
                samples,
            }
        })
        .collect();

    let now = chrono::Local::now();
    let doc = LdDocument {
        driver: "Driver".to_string(),
        vehicle: "SDM26".to_string(),
        venue: "Track".to_string(),
        comment: format!("{} channels, logger firmware export", channels.len()),
        event_name: "Full Data Session".to_string(),
        event_session: "All channels".to_string(),
        event_comment: format!("{} channels", channels.len()),
        date: now.format("%d/%m/%Y").to_string(),
        time: now.format("%H:%M:%S").to_string(),
        channels,
    };
    doc.to_bytes()
}

fn is_excluded(name: &str) -> bool {
    EXCLUDED_CHANNELS
        .iter()
        .any(|e| e.eq_ignore_ascii_case(name))
}

/// Average sample interval from the `timestamp` column (milliseconds),
fn infer_freq_hz(parsed: &ParsedLog) -> Option<u16> {
    let idx = parsed
        .columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case("timestamp"))?;

    let first: f64 = parsed.rows.first()?.get(idx)?.parse().ok()?;
    let last: f64 = parsed.rows.last()?.get(idx)?.parse().ok()?;
    let n = parsed.rows.len();
    if n < 2 || last <= first {
        return None;
    }

    let avg_dt_ms = (last - first) / (n - 1) as f64;
    if avg_dt_ms <= 0.0 {
        return None;
    }
    Some((1000.0 / avg_dt_ms).round().max(1.0) as u16)
}

struct LdDocument {
    driver: String,
    vehicle: String,
    venue: String,
    comment: String,
    event_name: String,
    event_session: String,
    event_comment: String,
    /// `dd/mm/yyyy`
    date: String,
    /// `HH:MM:SS`
    time: String,
    channels: Vec<Channel>,
}

impl LdDocument {
    fn to_bytes(&self) -> Vec<u8> {
        let numchannels = self.channels.len() as u32;
        let firstchannelpos = if numchannels > 0 {
            (LOG_HEADER_SIZE + EVENT_SIZE) as u32
        } else {
            0
        };
        let firstchanneldatapos = if numchannels > 0 {
            firstchannelpos + numchannels * CHANNEL_HEADER_SIZE as u32
        } else {
            0
        };

        let header_len = LOG_HEADER_SIZE + EVENT_SIZE + self.channels.len() * CHANNEL_HEADER_SIZE;
        let mut buf = vec![0u8; header_len];

        // Header
        put_u32(&mut buf, 0, 64); // id
        put_u32(&mut buf, 8, firstchannelpos);
        put_u32(&mut buf, 12, firstchanneldatapos);
        put_u32(&mut buf, 36, LOG_HEADER_SIZE as u32); // eventpos
        put_u32(&mut buf, 66, 1_000_000); // sig1
        put_u32(&mut buf, 70, 12007); // serial
        put_str(&mut buf, 74, 8, "ADL");
        put_u16(&mut buf, 82, 420); // version
        put_u16(&mut buf, 84, 128); // sig2
        put_u32(&mut buf, 86, numchannels);
        put_str(&mut buf, 94, 16, &self.date);
        put_str(&mut buf, 126, 16, &self.time);
        put_str(&mut buf, 158, 64, &self.driver);
        put_str(&mut buf, 222, 64, &self.vehicle);
        put_str(&mut buf, 350, 64, &self.venue);
        put_str(&mut buf, 1572, 64, &self.comment);

        // Event
        let ev = LOG_HEADER_SIZE;
        put_str(&mut buf, ev, 64, &self.event_name);
        put_str(&mut buf, ev + 64, 64, &self.event_session);
        put_str(&mut buf, ev + 128, 1024, &self.event_comment);
        put_u16(&mut buf, ev + 1152, 0); // venuepos

        // Channels + sample data
        let mut sample_data = Vec::new();
        let mut data_cursor = firstchanneldatapos;
        for (i, ch) in self.channels.iter().enumerate() {
            let pos = firstchannelpos as usize + i * CHANNEL_HEADER_SIZE;
            let prevpos = if i == 0 {
                0
            } else {
                (firstchannelpos as usize + (i - 1) * CHANNEL_HEADER_SIZE) as u32
            };
            let nextpos = if i + 1 < self.channels.len() {
                (firstchannelpos as usize + (i + 1) * CHANNEL_HEADER_SIZE) as u32
            } else {
                0
            };
            let numsamples = ch.samples.len() as u32;

            put_u32(&mut buf, pos, prevpos);
            put_u32(&mut buf, pos + 4, nextpos);
            put_u32(&mut buf, pos + 8, data_cursor);
            put_u32(&mut buf, pos + 12, numsamples);
            put_u16(&mut buf, pos + 16, 8000 + i as u16); // id
            put_u16(&mut buf, pos + 18, 0x0007); // datatype: float
            put_u16(&mut buf, pos + 20, 4); // datasize: 4 bytes
            put_u16(&mut buf, pos + 22, ch.freq_hz);
            put_i16(&mut buf, pos + 24, 0); // shift
            put_i16(&mut buf, pos + 26, 1); // multiplier
            put_i16(&mut buf, pos + 28, 1); // scale
            put_i16(&mut buf, pos + 30, 0); // decplaces
            put_str(&mut buf, pos + 32, 32, &ch.name);
            put_str(&mut buf, pos + 64, 8, &ch.short_name);
            put_str(&mut buf, pos + 72, 12, &ch.units);

            for &s in &ch.samples {
                sample_data.extend_from_slice(&s.to_le_bytes());
            }
            data_cursor += numsamples * 4;
        }

        buf.extend_from_slice(&sample_data);
        buf
    }
}

fn put_u32(buf: &mut [u8], at: usize, v: u32) {
    buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u16(buf: &mut [u8], at: usize, v: u16) {
    buf[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_i16(buf: &mut [u8], at: usize, v: i16) {
    buf[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

/// Writes `s` into `buf[at..at+len]`, truncated to `len` bytes and
/// zero-padded (the buffer starts zeroed, so padding is a no-op).
fn put_str(buf: &mut [u8], at: usize, len: usize, s: &str) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(len);
    buf[at..at + n].copy_from_slice(&bytes[..n]);
}

struct ChannelMeta {
    display: &'static str,
    short: &'static str,
    units: &'static str,
    convert: fn(f64) -> f64,
}

fn identity(v: f64) -> f64 {
    v
}

/// Channel metadata + calibration, keyed by our CSV column name. Falls back
/// to the name itself (short name truncated to 8 bytes, no units, no
/// conversion) for anything not in the table -- still a valid MoTeC channel,
/// just without curated units.
fn lookup(name: &str) -> ChannelMeta {
    match name {
        "p_F_brake" => ChannelMeta {
            display: "p_F_brake",
            short: "p_F_brake",
            units: "kPa",
            convert: identity,
        },
        "p_R_brake" => ChannelMeta {
            display: "p_R_brake",
            short: "p_R_brake",
            units: "kPa",
            convert: identity,
        },
        "Steering" => ChannelMeta {
            display: "Steering",
            short: "Steering",
            units: "deg",
            convert: |v| 0.084769 * (v - 1430.0),
        },
        "l_FL_damper" => ChannelMeta {
            display: "l_FL_damper",
            short: "l_FL_damper",
            units: "mm",
            convert: |v| -0.018586 * (v - 1311.0),
        },
        "l_FR_damper" => ChannelMeta {
            display: "l_FR_damper",
            short: "l_FR_damper",
            units: "mm",
            convert: |v| -0.018444 * (v - 1324.0),
        },
        "l_RR_damper" => ChannelMeta {
            display: "l_RR_damper",
            short: "l_RR_damper",
            units: "mm",
            convert: |v| -0.018498 * (v - 1370.0),
        },
        "l_RL_damper" => ChannelMeta {
            display: "l_RL_damper",
            short: "l_RL_damper",
            units: "mm",
            convert: |v| -0.018600 * (v - 1403.0),
        },
        // Already converted to physical units by the firmware (see
        // logging.rs's PowerColumns) -- identity here, not devices.py's
        // factor, to avoid scaling twice.
        "amp_Batt" => ChannelMeta {
            display: "amp_Batt",
            short: "amp_Batt",
            units: "A",
            convert: identity,
        },
        "v_batt" => ChannelMeta {
            display: "v_batt",
            short: "v_batt",
            units: "V",
            convert: identity,
        },
        "a_Lat" => ChannelMeta {
            display: "a_Lat",
            short: "a_Lat",
            units: "G",
            convert: |v| (v * 0.122) / 1000.0,
        },
        "a_Long" => ChannelMeta {
            display: "a_Long",
            short: "a_Long",
            units: "g",
            convert: |v| (v * 0.122) / 1000.0,
        },
        "a_Vert" => ChannelMeta {
            display: "a_Vert",
            short: "a_Vert",
            units: "g",
            convert: |v| (v * 0.122) / 1000.0,
        },
        "r_Pitch" => ChannelMeta {
            display: "r_Pitch",
            short: "r_Pitch",
            units: "deg/s",
            convert: |v| (v * 17.50) / 1000.0,
        },
        "r_Roll" => ChannelMeta {
            display: "r_Roll",
            short: "r_Roll",
            units: "deg/s",
            convert: |v| (v * 17.50) / 1000.0,
        },
        "r_Yaw" => ChannelMeta {
            display: "r_Yaw",
            short: "r_Yaw",
            units: "deg/s",
            convert: |v| (v * 17.50) / 1000.0,
        },
        "FR_SG" => ChannelMeta {
            display: "f_FR_PartTBD",
            short: "f_FR_PartTBD",
            units: "raw",
            convert: identity,
        },
        "FL_SG" => ChannelMeta {
            display: "f_FL_PartTBD",
            short: "f_FL_PartTBD",
            units: "",
            convert: |v| -11052026.1 * v + 2606.22253,
        },
        "RL_SG" => ChannelMeta {
            display: "f_RL_PartTBD",
            short: "f_RL_PartTBD",
            units: "",
            convert: |v| -1401922.44 * v + 92026.0137,
        },
        "RR_SG" => ChannelMeta {
            display: "f_RR_PartTBD",
            short: "f_RR_PartTBD",
            units: "",
            convert: identity,
        },
        "t_FL_amb" => ChannelMeta {
            display: "t_FL_amb",
            short: "t_FL_amb",
            units: "C",
            convert: identity,
        },
        "FLW_OBJ" => ChannelMeta {
            display: "FL Wheel Object",
            short: "FLW_Obj",
            units: "",
            convert: identity,
        },
        "r_FL_wheel" => ChannelMeta {
            display: "r_FL_wheel",
            short: "r_FL_wheel",
            units: "rpm",
            convert: identity,
        },
        "t_FR_amb" => ChannelMeta {
            display: "t_FR_amb",
            short: "t_FR_amb",
            units: "C",
            convert: identity,
        },
        "FRW_OBJ" => ChannelMeta {
            display: "FR Wheel Object",
            short: "FRW_Obj",
            units: "",
            convert: identity,
        },
        "r_FR_wheel" => ChannelMeta {
            display: "r_FR_wheel",
            short: "r_FR_wheel",
            units: "rpm",
            convert: identity,
        },
        "t_RR_amb" => ChannelMeta {
            display: "t_RR_amb",
            short: "t_RR_amb",
            units: "C",
            convert: identity,
        },
        "RRW_OBJ" => ChannelMeta {
            display: "RR Wheel Object",
            short: "RRW_Obj",
            units: "",
            convert: identity,
        },
        "r_RR_wheel" => ChannelMeta {
            display: "r_RR_wheel",
            short: "r_RR_wheel",
            units: "rpm",
            convert: identity,
        },
        "t_RL_amb" => ChannelMeta {
            display: "t_RL_amb",
            short: "t_RL_amb",
            units: "C",
            convert: identity,
        },
        "RLW_OBJ" => ChannelMeta {
            display: "RL Wheel Object",
            short: "RLW_Obj",
            units: "",
            convert: identity,
        },
        "r_RL_wheel" => ChannelMeta {
            display: "r_RL_wheel",
            short: "r_RL_wheel",
            units: "rpm",
            convert: identity,
        },
        "BRAKE_FLUID" => ChannelMeta {
            display: "Brake Fluid",
            short: "BrkFluid",
            units: "",
            convert: identity,
        },
        "THROTTLE_LOAD" => ChannelMeta {
            display: "Throttle Load",
            short: "Throttle",
            units: "%",
            convert: identity,
        },
        "BRAKE_LOAD" => ChannelMeta {
            display: "Brake Load",
            short: "Brake",
            units: "%",
            convert: identity,
        },
        "DRS" => ChannelMeta {
            display: "DRS",
            short: "DRS",
            units: "",
            convert: identity,
        },
        "gps_Long" => ChannelMeta {
            display: "gps_Long",
            short: "gps_Long",
            units: "deg",
            convert: identity,
        },
        "gps_Lat" => ChannelMeta {
            display: "gps_Lat",
            short: "gps_Lat",
            units: "deg",
            convert: identity,
        },
        "v_car_gps" => ChannelMeta {
            display: "v_car_gps",
            short: "v_car_gps",
            units: "km/h",
            convert: identity,
        },
        "gps_fix" => ChannelMeta {
            display: "gps_fix",
            short: "gps_fix",
            units: "",
            convert: identity,
        },
        "r_engine" => ChannelMeta {
            display: "r_engine",
            short: "r_engine",
            units: "rpm",
            convert: identity,
        },
        "t_eng_coolant" => ChannelMeta {
            display: "t_eng_coolant",
            short: "t_eng_coolant",
            units: "C",
            convert: |v| v - 50.0,
        },
        "t_oil" => ChannelMeta {
            display: "t_oil",
            short: "t_oil",
            units: "C",
            convert: |v| v - 50.0,
        },
        "p_oil" => ChannelMeta {
            display: "p_oil",
            short: "p_oil",
            units: "kPa",
            convert: identity,
        },
        "neutral" => ChannelMeta {
            display: "neutral",
            short: "neutral",
            units: "",
            convert: identity,
        },
        // Already scaled by firmware (config.json sets scale=0.01) -- identity here.
        "Lamb_1" => ChannelMeta {
            display: "Lamb_1",
            short: "Lamb_1",
            units: "Lambda",
            convert: identity,
        },
        "%_TPS" => ChannelMeta {
            display: "%_TPS",
            short: "%_TPS",
            units: "%",
            convert: identity,
        },
        "n-Gear" => ChannelMeta {
            display: "n-Gear",
            short: "n-Gear",
            units: "",
            convert: identity,
        },
        "v_trans_out" => ChannelMeta {
            display: "v_trans_out",
            short: "v_trans_out",
            units: "km/h",
            convert: |v| 0.1 * v,
        },
        "%_APS_main" => ChannelMeta {
            display: "%_APS_main",
            short: "%_APS_main",
            units: "%",
            convert: |v| 0.1 * v,
        },
        "p_Fuel" => ChannelMeta {
            display: "p_Fuel",
            short: "p_Fuel",
            units: "kPa",
            convert: identity,
        },
        "n_knock_count" => ChannelMeta {
            display: "Knock Count Global",
            short: "Knock_Cnt",
            units: "",
            convert: identity,
        },
        "d_ign_angle" => ChannelMeta {
            display: "Ignition Angle",
            short: "Ign_Angle",
            units: "deg",
            convert: identity,
        },
        "%_ign_cut" => ChannelMeta {
            display: "Ignition Cut",
            short: "Ign_Cut",
            units: "%",
            convert: identity,
        },
        "%_fuel_cut" => ChannelMeta {
            display: "Fuel Cut",
            short: "Fuel_Cut",
            units: "%",
            convert: identity,
        },
        "r_idle_target" => ChannelMeta {
            display: "Idle Target",
            short: "Idle_Tgt",
            units: "rpm",
            convert: identity,
        },
        "%_lambda_corr" => ChannelMeta {
            display: "CL Lambda Fuel Corr",
            short: "Lam_Corr",
            units: "%",
            convert: identity,
        },
        "Lambda_Target_Err" => ChannelMeta {
            display: "Lambda Target Error",
            short: "Lam_Err",
            units: "",
            convert: identity,
        },
        "in_gear" => ChannelMeta {
            display: "In Driving Gear",
            short: "In_Gear",
            units: "",
            convert: identity,
        },
        "upshift_act" => ChannelMeta {
            display: "Aux 5 - UpShift Actuator",
            short: "UpShift",
            units: "",
            convert: identity,
        },
        "downshift_act" => ChannelMeta {
            display: "Aux 8 - DownShift Actuator",
            short: "DnShift",
            units: "",
            convert: identity,
        },
        "launch_ctrl_stat" => ChannelMeta {
            display: "Launch Control Status",
            short: "Launch",
            units: "",
            convert: identity,
        },
        "eng_fan_1" => ChannelMeta {
            display: "Engine Fan 1",
            short: "Eng_Fan1",
            units: "",
            convert: identity,
        },
        "%_fuel_left" => ChannelMeta {
            display: "Fuel Left",
            short: "Fuel_Lvl",
            units: "%",
            convert: identity,
        },
        "t_fuel_accel" => ChannelMeta {
            display: "t_fuel_accel",
            short: "t_fuel_accel",
            units: "ms",
            convert: |v| 0.001 * v,
        },
        "acc_distance" => ChannelMeta {
            display: "acc_distance",
            short: "acc_distance",
            units: "km",
            convert: |v| 0.1 * v,
        },
        "p_MAP" => ChannelMeta {
            display: "p_MAP",
            short: "p_MAP",
            units: "kPa",
            convert: identity,
        },
        "t_MAT" => ChannelMeta {
            display: "t_MAT",
            short: "t_MAT",
            units: "C",
            convert: |v| v - 50.0,
        },
        "a_Lat_ecu" => ChannelMeta {
            display: "a_Lat_ecu",
            short: "a_Lat_ecu",
            units: "g",
            convert: unwrap_u16_then_milli,
        },
        "a_Long_ecu" => ChannelMeta {
            display: "a_Long_ecu",
            short: "a_Long_ecu",
            units: "g",
            convert: unwrap_u16_then_milli,
        },
        "a_Vert_ecu" => ChannelMeta {
            display: "a_Vert_ecu",
            short: "a_Vert_ecu",
            units: "g",
            convert: unwrap_u16_then_milli,
        },
        "TESTNO" => ChannelMeta {
            display: "Test Number",
            short: "TestNo",
            units: "",
            convert: identity,
        },
        "DTC_FLW" => ChannelMeta {
            display: "DTC FL Wheel",
            short: "DTC_FLW",
            units: "",
            convert: identity,
        },
        "DTC_FRW" => ChannelMeta {
            display: "DTC FR Wheel",
            short: "DTC_FRW",
            units: "",
            convert: identity,
        },
        "DTC_RLW" => ChannelMeta {
            display: "DTC RL Wheel",
            short: "DTC_RLW",
            units: "",
            convert: identity,
        },
        "DTC_RRW" => ChannelMeta {
            display: "DTC RR Wheel",
            short: "DTC_RRW",
            units: "",
            convert: identity,
        },
        "DTC_FLSG" => ChannelMeta {
            display: "DTC FL Strain",
            short: "DTC_FLSG",
            units: "",
            convert: identity,
        },
        "DTC_FRSG" => ChannelMeta {
            display: "DTC FR Strain",
            short: "DTC_FRSG",
            units: "",
            convert: identity,
        },
        "DTC_RLSG" => ChannelMeta {
            display: "DTC RL Strain",
            short: "DTC_RLSG",
            units: "",
            convert: identity,
        },
        "DTC_RRSG" => ChannelMeta {
            display: "DTC RR Strain",
            short: "DTC_RRSG",
            units: "",
            convert: identity,
        },
        "DTC_IMU" => ChannelMeta {
            display: "DTC IMU",
            short: "DTC_IMU",
            units: "",
            convert: identity,
        },
        "GPS_0_" => ChannelMeta {
            display: "GPS 0",
            short: "GPS_0",
            units: "",
            convert: identity,
        },
        "GPS_1_" => ChannelMeta {
            display: "GPS 1",
            short: "GPS_1",
            units: "",
            convert: identity,
        },
        "FLT_TTA" => ChannelMeta {
            display: "FL Tire Temp A",
            short: "FLT_TTA",
            units: "",
            convert: identity,
        },
        "FLT_TTB" => ChannelMeta {
            display: "FL Tire Temp B",
            short: "FLT_TTB",
            units: "",
            convert: identity,
        },
        "FRT_TTA" => ChannelMeta {
            display: "FR Tire Temp A",
            short: "FRT_TTA",
            units: "",
            convert: identity,
        },
        "FRT_TTB" => ChannelMeta {
            display: "FR Tire Temp B",
            short: "FRT_TTB",
            units: "",
            convert: identity,
        },
        "RLT_TTA" => ChannelMeta {
            display: "RL Tire Temp A",
            short: "RLT_TTA",
            units: "",
            convert: identity,
        },
        "RLT_TTB" => ChannelMeta {
            display: "RL Tire Temp B",
            short: "RLT_TTB",
            units: "",
            convert: identity,
        },
        "RRT_TTA" => ChannelMeta {
            display: "RR Tire Temp A",
            short: "RRT_TTA",
            units: "",
            convert: identity,
        },
        "RRT_TTB" => ChannelMeta {
            display: "RR Tire Temp B",
            short: "RRT_TTB",
            units: "",
            convert: identity,
        },
        "CH_COUNT" => ChannelMeta {
            display: "Channel Count",
            short: "CH_Count",
            units: "",
            convert: identity,
        },
        "FR_Wheel_Speed" => ChannelMeta {
            display: "FR Wheel Speed",
            short: "FR_wspd",
            units: "km/h",
            convert: identity,
        },
        "FL_Wheel_Speed" => ChannelMeta {
            display: "FL Wheel Speed",
            short: "FL_wspd",
            units: "km/h",
            convert: identity,
        },
        _ => ChannelMeta {
            display: leak(name),
            short: leak(&name[..name.len().min(8)]),
            units: "",
            convert: identity,
        },
    }
}

fn unwrap_u16_then_milli(v: f64) -> f64 {
    (if v >= 32768.0 { v - 65536.0 } else { v }) * 0.001
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_log() -> ParsedLog {
        ParsedLog {
            columns: vec![
                "timestamp".into(),
                "Steering".into(),
                "amp_Batt".into(),
                "some_unmapped_channel".into(),
            ],
            rows: vec![
                vec!["0".into(), "1430".into(), "12.5".into(), "3".into()],
                vec!["50".into(), "1440".into(), "12.4".into(), "4".into()],
                vec!["100".into(), "1450".into(), "12.3".into(), "5".into()],
            ],
        }
    }

    #[test]
    fn infers_20hz_from_50ms_steps() {
        assert_eq!(infer_freq_hz(&sample_log()), Some(20));
    }

    #[test]
    fn excludes_timestamp_channel_and_keeps_others() {
        let bytes = build_ld(&sample_log());
        // 3 channels expected: Steering, amp_Batt, some_unmapped_channel
        // (timestamp excluded). numchannels lives at header offset 86.
        let numchannels = u32::from_le_bytes(bytes[86..90].try_into().unwrap());
        assert_eq!(numchannels, 3);
    }

    #[test]
    fn header_pointers_and_channel_layout_are_self_consistent() {
        let bytes = build_ld(&sample_log());

        let firstchannelpos = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let firstchanneldatapos = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        assert_eq!(firstchannelpos, LOG_HEADER_SIZE + EVENT_SIZE);
        assert_eq!(
            firstchanneldatapos,
            firstchannelpos + 3 * CHANNEL_HEADER_SIZE
        );

        // Walk the channel linked list via nextpos and check each channel's
        // own datapos/numsamples/name against what build_ld put there.
        let expected = ["Steering", "amp_Batt", "some_unmapped_channel"];
        let mut pos = firstchannelpos;
        let mut expected_datapos = firstchanneldatapos;
        for (i, name) in expected.iter().enumerate() {
            let prevpos = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
            let nextpos = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
            let datapos = u32::from_le_bytes(bytes[pos + 8..pos + 12].try_into().unwrap());
            let numsamples = u32::from_le_bytes(bytes[pos + 12..pos + 16].try_into().unwrap());
            let datatype = u16::from_le_bytes(bytes[pos + 18..pos + 20].try_into().unwrap());
            let freq = u16::from_le_bytes(bytes[pos + 22..pos + 24].try_into().unwrap());
            let stored_name = std::str::from_utf8(&bytes[pos + 32..pos + 32 + name.len()]).unwrap();

            assert_eq!(datapos as usize, expected_datapos);
            assert_eq!(numsamples, 3);
            assert_eq!(datatype, 0x0007);
            assert_eq!(freq, 20);
            assert_eq!(stored_name, *name);
            assert_eq!(
                prevpos as usize,
                if i == 0 { 0 } else { pos - CHANNEL_HEADER_SIZE }
            );
            assert_eq!(
                nextpos as usize,
                if i + 1 < expected.len() {
                    pos + CHANNEL_HEADER_SIZE
                } else {
                    0
                }
            );

            expected_datapos += numsamples as usize * 4;
            pos = if nextpos == 0 { pos } else { nextpos as usize };
        }

        assert_eq!(bytes.len(), expected_datapos);
    }

    #[test]
    fn steering_channel_applies_calibration() {
        let bytes = build_ld(&sample_log());
        let firstchannelpos = LOG_HEADER_SIZE + EVENT_SIZE;
        let firstchanneldatapos = firstchannelpos + 3 * CHANNEL_HEADER_SIZE;

        // Steering is the first channel: samples are raw 1430/1440/1450 ->
        // 0.084769 * (v - 1430) per devices.py.
        let sample = |i: usize| {
            let at = firstchanneldatapos + i * 4;
            f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
        };
        assert!((sample(0) - 0.0).abs() < 1e-4);
        assert!((sample(1) - 0.84769).abs() < 1e-3);
        assert!((sample(2) - 1.69538).abs() < 1e-3);
    }

    #[test]
    fn already_scaled_channel_gets_identity_conversion() {
        let bytes = build_ld(&sample_log());
        let firstchannelpos = LOG_HEADER_SIZE + EVENT_SIZE;
        // amp_Batt is the second channel: firmware already scaled it, so the
        // MoTeC table must NOT apply factor again.
        let datapos = u32::from_le_bytes(
            bytes[firstchannelpos + CHANNEL_HEADER_SIZE + 8
                ..firstchannelpos + CHANNEL_HEADER_SIZE + 12]
                .try_into()
                .unwrap(),
        ) as usize;
        let v = f32::from_le_bytes(bytes[datapos..datapos + 4].try_into().unwrap());
        assert!((v - 12.5).abs() < 1e-4);
    }

    #[test]
    fn unmapped_channel_falls_back_to_its_own_name() {
        let meta = lookup("some_unmapped_channel");
        assert_eq!(meta.display, "some_unmapped_channel");
        assert_eq!(meta.short, "some_unm");
        assert_eq!(meta.units, "");
    }
}
