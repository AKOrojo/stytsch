use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::db::Database;
use crate::schema::SearchFilter;

/// Result from fzf search.
pub enum FzfResult {
    /// User pressed Enter — execute the command immediately.
    Execute(String),
    /// User pressed Right Arrow — paste into command line for editing.
    Edit(String),
    /// User cancelled (Esc).
    Cancelled,
}

pub fn which_fzf() -> Option<std::path::PathBuf> {
    Command::new("where")
        .arg("fzf")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
                path.lines().next().map(std::path::PathBuf::from)
            } else {
                None
            }
        })
}

pub fn search_with_fzf(db: &Database, _query: Option<&str>) -> Result<FzfResult> {
    let entries = db.search(&SearchFilter::new(5000))?;

    // Deduplicate, keep most recent.
    let mut seen = std::collections::HashSet::new();
    let mut unique: Vec<&str> = Vec::new();
    for h in &entries {
        if seen.insert(&h.command) {
            unique.push(&h.command);
        }
    }

    let mut fzf = Command::new("fzf")
        .args([
            "--height=40%",
            "--reverse",
            "--no-sort",
            "--prompt=stytsch> ",
            "--header=Enter: run | Tab: edit | Esc: cancel",
            "--expect=tab",
            "--bind=tab:accept",
            "--print-query",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    if let Some(mut stdin) = fzf.stdin.take() {
        for cmd in &unique {
            writeln!(stdin, "{cmd}")?;
        }
    }

    let output = fzf.wait_with_output()?;

    // fzf exit codes: 0=selected, 1=no match, 2=error, 130=interrupted (Esc/Ctrl+C).
    // With --print-query, fzf still outputs the query on exit code 1.
    // Only treat 130 (Esc) and 2 (error) as cancelled.
    let code = output.status.code().unwrap_or(130);
    if code == 130 || code == 2 {
        return Ok(FzfResult::Cancelled);
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut lines = raw.lines();

    // With --print-query and --expect, fzf outputs 3 lines:
    //   Line 1: the query (what user typed in the search box)
    //   Line 2: the key that was pressed (empty = Enter, "right" = →)
    //   Line 3: the selected item (empty if nothing matched)
    let query = lines.next().unwrap_or("").trim().to_string();
    let key = lines.next().unwrap_or("").trim().to_string();
    let selected = lines.next().unwrap_or("").trim().to_string();

    // Use the selected item if available, otherwise fall back to the typed query.
    let command = if !selected.is_empty() {
        selected
    } else if !query.is_empty() {
        query
    } else {
        return Ok(FzfResult::Cancelled);
    };

    if key == "tab" {
        Ok(FzfResult::Edit(command))
    } else {
        Ok(FzfResult::Execute(command))
    }
}
