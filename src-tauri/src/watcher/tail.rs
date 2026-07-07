use crate::db::queries;
use rusqlite::Connection;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Reads any bytes appended to `path` since the last recorded offset, returning complete
/// lines only. A trailing incomplete line (the watcher fired mid-flush, a real possibility
/// since Claude Code appends while we're reading) is buffered back into `ingest_state` for
/// next time rather than parsed prematurely. Offset/partial-line bookkeeping is updated in
/// the same call so a crash between reading and processing can't silently skip bytes.
pub fn read_new_lines(conn: &Connection, path: &Path) -> anyhow::Result<Vec<String>> {
    let path_str = path.to_string_lossy().to_string();
    let state = queries::get_ingest_state(conn, &path_str)?;

    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();

    // If the file is shorter than our recorded offset (shouldn't happen for Claude Code's
    // append-only logs, but never trust external state), restart from 0 rather than seek
    // past EOF and silently miss everything that follows.
    let start_offset = if (state.byte_offset as u64) > file_len {
        0
    } else {
        state.byte_offset as u64
    };

    file.seek(SeekFrom::Start(start_offset))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    let new_text = String::from_utf8_lossy(&buf);
    let combined = format!("{}{}", state.partial_line, new_text);

    let mut lines: Vec<String> = combined.split('\n').map(String::from).collect();
    // Last element is "" if `combined` ended in `\n`, or an incomplete trailing line
    // otherwise — either way it must not be parsed yet.
    let trailing_partial = lines.pop().unwrap_or_default();

    let new_offset = start_offset + buf.len() as u64;
    let mtime = file
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    queries::set_ingest_state(conn, &path_str, new_offset as i64, &trailing_partial, mtime)?;

    Ok(lines.into_iter().filter(|l| !l.trim().is_empty()).collect())
}
