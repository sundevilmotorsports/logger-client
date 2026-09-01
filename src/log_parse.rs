//! Decodes a downloaded binary log into columns + stringified rows for CSV
//! export. The wire format lives in `sdm_utils::logfmt`.

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

/// Serializes a parsed log to CSV bytes.
pub fn to_csv(parsed: &ParsedLog) -> Vec<u8> {
    let mut out = parsed.columns.join(",");
    out.push('\n');
    for row in &parsed.rows {
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out.into_bytes()
}
