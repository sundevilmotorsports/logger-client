/// A parsed log: column names and decoded row values. Format matches the
/// firmware's `src/logging.rs`: header of [num_columns, (name_len, name,
/// type tag)...], then fixed-width rows back to back.
pub struct ParsedLog {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

enum ColType {
    Float,
    Raw(u8),
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> anyhow::Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| anyhow::anyhow!("log offset overflow"))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| anyhow::anyhow!("truncated log"))?;
        self.pos = end;
        Ok(slice)
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
}

pub fn parse_log(
    raw: &[u8],
    mut on_progress: impl FnMut(usize, usize),
) -> anyhow::Result<ParsedLog> {
    let mut cur = Cursor { data: raw, pos: 0 };

    let num_cols = cur.take(1)?[0] as usize;
    let mut columns = Vec::with_capacity(num_cols);
    let mut types = Vec::with_capacity(num_cols);
    for _ in 0..num_cols {
        let name_len = cur.take(1)?[0] as usize;
        let name = String::from_utf8_lossy(cur.take(name_len)?).into_owned();
        let tag = cur.take(1)?[0];
        columns.push(name);
        types.push(if tag == 0 {
            ColType::Float
        } else {
            ColType::Raw(tag)
        });
    }

    let row_width: usize = types
        .iter()
        .map(|t| match t {
            ColType::Float => 4,
            ColType::Raw(n) => *n as usize,
        })
        .sum();
    if row_width == 0 {
        return Err(anyhow::anyhow!("log schema has no columns"));
    }

    // Bounded to ~50 updates regardless of file size
    let progress_interval = (cur.remaining() / row_width / 50).max(1);

    let mut rows = Vec::new();
    while cur.remaining() >= row_width {
        let mut row = Vec::with_capacity(num_cols);
        for ty in &types {
            let value = match ty {
                ColType::Float => {
                    let bytes: [u8; 4] = cur.take(4)?.try_into().unwrap();
                    format!("{}", f32::from_le_bytes(bytes))
                }
                ColType::Raw(n) => {
                    let bytes = cur.take(*n as usize)?;
                    let mut buf = [0u8; 8];
                    buf[..bytes.len()].copy_from_slice(bytes);
                    u64::from_le_bytes(buf).to_string()
                }
            };
            row.push(value);
        }
        rows.push(row);
        if rows.len() % progress_interval == 0 {
            on_progress(cur.pos, raw.len());
        }
    }
    on_progress(cur.pos, raw.len());

    Ok(ParsedLog { columns, rows })
}

/// Serializes a parsed log to CSV bytes
pub fn to_csv(parsed: &ParsedLog) -> Vec<u8> {
    let mut out = parsed.columns.join(",");
    out.push('\n');
    for row in &parsed.rows {
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out.into_bytes()
}
