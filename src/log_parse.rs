/// A parsed log: header names and row fields, split out of the firmware's
/// CSV format (ASCII only, no quoting — a plain `split(',')` is exact).
pub struct ParsedLog {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub malformed: usize,
}

pub fn parse_csv(raw: &[u8]) -> anyhow::Result<ParsedLog> {
    let text = std::str::from_utf8(raw)?;
    let mut lines = text.lines();
    let header = lines.next().ok_or_else(|| anyhow::anyhow!("empty log"))?;
    let columns: Vec<String> = header.split(',').map(str::to_string).collect();

    let mut rows = Vec::new();
    let mut malformed = 0;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<String> = line.split(',').map(str::to_string).collect();
        if fields.len() == columns.len() {
            rows.push(fields);
        } else {
            malformed += 1;
        }
    }

    Ok(ParsedLog {
        columns,
        rows,
        malformed,
    })
}

/// Re-serializes a parsed log back to clean CSV bytes (drops malformed rows).
pub fn to_csv(parsed: &ParsedLog) -> Vec<u8> {
    let mut out = parsed.columns.join(",");
    out.push('\n');
    for row in &parsed.rows {
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out.into_bytes()
}
