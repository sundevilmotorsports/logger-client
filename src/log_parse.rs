//! Decodes a downloaded binary log into columns + stringified rows, and
//! serializes that to a unit-converted CSV. The wire format lives in
//! `sdm_utils::logfmt`; the unit calibration lives in `crate::motec`.

use sdm_utils::logfmt::Schema;

/// A parsed log: column names and decoded row values.
pub struct ParsedLog {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn parse_log(
    raw: &[u8],
    mut on_progress: impl FnMut(usize, usize),
) -> anyhow::Result<ParsedLog> {
    let mut cursor = raw;
    let schema = Schema::decode_header(&mut cursor).map_err(|e| anyhow::anyhow!("{e}"))?;

    let width = schema.row_width();
    if width == 0 {
        return Err(anyhow::anyhow!("log schema has no columns"));
    }

    let columns = schema.columns.iter().map(|c| c.name.clone()).collect();
    let header_len = raw.len() - cursor.len();
    let total = raw.len();

    // Bounded to ~50 progress callbacks regardless of file size.
    let interval = (cursor.len() / width / 50).max(1);

    let mut rows = Vec::new();
    for (i, row) in schema.rows(cursor).enumerate() {
        rows.push(row.values().map(|v| v.render()).collect());
        if (i + 1) % interval == 0 {
            on_progress(header_len + (i + 1) * width, total);
        }
    }
    on_progress(total, total);

    Ok(ParsedLog { columns, rows })
}

/// Serializes a parsed log to CSV bytes. Values are converted to physical
/// units through the same calibration table [`crate::motec::build_ld`] uses
/// (see `crate::motec::lookup`), so the CSV and the .ld always agree; the
/// header names each column's units, e.g. `a_Lat (G)`.
pub fn to_csv(parsed: &ParsedLog) -> Vec<u8> {
    let metas: Vec<_> = parsed
        .columns
        .iter()
        .map(|c| crate::motec::lookup(c))
        .collect();

    let mut out = parsed
        .columns
        .iter()
        .zip(&metas)
        .map(|(name, meta)| {
            if meta.units.is_empty() {
                name.clone()
            } else {
                format!("{name} ({})", meta.units)
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    out.push('\n');

    for row in &parsed.rows {
        let line = row
            .iter()
            .zip(&metas)
            .map(|(cell, meta)| {
                let raw: f64 = cell.parse().unwrap_or(0.0);
                format_value((meta.convert)(raw))
            })
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&line);
        out.push('\n');
    }
    out.into_bytes()
}

/// Renders a converted value the way a spreadsheet expects: whole numbers
/// without a trailing `.0`, otherwise fixed to 6 decimals with trailing
/// zeros trimmed. Non-finite values (seen in corrupted log rows) render as
/// `NaN`/`inf`/`-inf` rather than being silently zeroed, so corruption stays
/// visible in the CSV instead of being masked.
fn format_value(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
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
            ],
        }
    }

    #[test]
    fn header_carries_units_for_mapped_channels_only() {
        let csv = String::from_utf8(to_csv(&sample_log())).unwrap();
        let header = csv.lines().next().unwrap();
        assert_eq!(
            header,
            "timestamp (s),Steering (deg),amp_Batt (A),some_unmapped_channel"
        );
    }

    #[test]
    fn values_match_the_ld_calibration() {
        let csv = String::from_utf8(to_csv(&sample_log())).unwrap();
        let mut lines = csv.lines().skip(1);

        // timestamp: ms -> s; Steering: 0.084769 * (v - 1430); amp_Batt:
        // already scaled by firmware, identity conversion.
        assert_eq!(lines.next().unwrap(), "0,0,12.5,3");
        assert_eq!(lines.next().unwrap(), "0.05,0.84769,12.4,4");
    }

    #[test]
    fn non_finite_values_stay_visible_instead_of_being_zeroed() {
        assert_eq!(format_value(f64::NAN), "NaN");
        assert_eq!(format_value(f64::INFINITY), "inf");
        assert_eq!(format_value(f64::NEG_INFINITY), "-inf");
    }

    #[test]
    fn whole_numbers_render_without_a_decimal_point() {
        assert_eq!(format_value(12.0), "12");
        assert_eq!(format_value(-3.0), "-3");
        assert_eq!(format_value(0.84769), "0.84769");
    }
}
