//! The history of each app (ADR-2610071122): numbered versions, why each was saved, how many are
//! kept, which files to copy, and a line diff to show what changed. Pure: the use case does the files.

use super::package::SKIP_DIRS;
use serde::{Deserialize, Serialize};

/// How many versions of one app are kept; older ones are dropped.
pub const KEEP_VERSIONS: usize = 50;
/// A text file longer than this many lines is not compared line by line.
const MAX_DIFF_LINES: usize = 4000;
/// Lines of unchanged text shown around each change.
const CONTEXT: usize = 2;

/// One saved version of an app.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Version {
    pub n: u64,
    /// Unix seconds.
    pub at: u64,
    /// What saved it: make-an-app, import, restore, promote or first-seen.
    pub by: String,
    pub why: String,
}

/// The number the next version gets.
pub fn next_number(log: &[Version]) -> u64 {
    log.iter().map(|v| v.n).max().unwrap_or(0) + 1
}

/// Drops the oldest versions beyond `keep`. Returns the numbers dropped, so their files can go too.
pub fn prune(log: &mut Vec<Version>, keep: usize) -> Vec<u64> {
    if log.len() <= keep {
        return Vec::new();
    }
    log.sort_by_key(|v| v.n);
    let cut = log.len() - keep;
    log.drain(..cut).map(|v| v.n).collect()
}

/// Whether a file belongs in a copy of an app: build output (target/, node_modules/, a Rust
/// app's Cargo.lock) and repositories stay out.
pub fn kept_in_copies(rel: &str) -> bool {
    !rel.split('/').any(|seg| SKIP_DIRS.contains(&seg) || seg == ".git") && rel.rsplit('/').next() != Some("Cargo.lock")
}

/// What `wardian promote` changed in the source folder.
#[derive(Debug, Default, PartialEq)]
pub struct Promoted {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

/// How a file differs between a version and the current app.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    /// In the current app, not in the version.
    Added,
    /// In the version, not in the current app.
    Removed,
    Changed,
    Same,
}

pub fn change(old: Option<&[u8]>, new: Option<&[u8]>) -> Change {
    match (old, new) {
        (None, Some(_)) => Change::Added,
        (Some(_), None) => Change::Removed,
        (Some(a), Some(b)) if a == b => Change::Same,
        _ => Change::Changed,
    }
}

/// A line diff from `old` to `new`: changed lines prefixed "-" and "+", a few unchanged lines
/// around each change prefixed " ", and "…" where unchanged lines are left out. None when either
/// side is not text, or too long to compare line by line.
pub fn line_diff(old: &[u8], new: &[u8]) -> Option<String> {
    let (old, new) = (std::str::from_utf8(old).ok()?, std::str::from_utf8(new).ok()?);
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    if a.len() > MAX_DIFF_LINES || b.len() > MAX_DIFF_LINES {
        return None;
    }
    // Longest common subsequence, by a table of suffix lengths. The table has (n+1)(m+1) cells of
    // 4 bytes; past 4 million cells (16 MB) the files are not compared line by line.
    let (n, m) = (a.len(), b.len());
    if (n + 1).saturating_mul(m + 1) > 4_000_000 {
        return None;
    }
    let mut lcs = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[at(i, j)] = if a[i] == b[j] { lcs[at(i + 1, j + 1)] + 1 } else { lcs[at(i + 1, j)].max(lcs[at(i, j + 1)]) };
        }
    }
    // Walk the table into (mark, line) pairs.
    let mut ops: Vec<(char, &str)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            ops.push((' ', a[i]));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[at(i + 1, j)] >= lcs[at(i, j + 1)]) {
            // Removals before additions, as diffs are read.
            ops.push(('-', a[i]));
            i += 1;
        } else {
            ops.push(('+', b[j]));
            j += 1;
        }
    }
    // Keep each change with CONTEXT unchanged lines on either side.
    let changed: Vec<usize> = ops.iter().enumerate().filter(|(_, (c, _))| *c != ' ').map(|(k, _)| k).collect();
    if changed.is_empty() {
        return Some(String::new());
    }
    let mut keep = vec![false; ops.len()];
    for &k in &changed {
        for slot in keep.iter_mut().take((k + CONTEXT + 1).min(ops.len())).skip(k.saturating_sub(CONTEXT)) {
            *slot = true;
        }
    }
    let mut out = String::new();
    let mut gap = false;
    for (k, (c, line)) in ops.iter().enumerate() {
        if keep[k] {
            if gap {
                out.push_str("…\n");
                gap = false;
            }
            out.push(*c);
            out.push_str(line);
            out.push('\n');
        } else {
            gap = true;
        }
    }
    if gap {
        out.push_str("…\n");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: u64) -> Version {
        Version { n, at: n, by: "make-an-app".into(), why: String::new() }
    }

    #[test]
    fn numbers_and_pruning() {
        assert_eq!(next_number(&[]), 1);
        let mut log: Vec<Version> = (1..=53).map(v).collect();
        assert_eq!(next_number(&log), 54);
        assert_eq!(prune(&mut log, KEEP_VERSIONS), vec![1, 2, 3]);
        assert_eq!(log.len(), 50);
        assert_eq!(log[0].n, 4);
        assert_eq!(next_number(&log), 54, "numbers never repeat after pruning");
        assert!(prune(&mut log, KEEP_VERSIONS).is_empty());
    }

    #[test]
    fn copies_leave_out_build_output() {
        assert!(kept_in_copies("apps/main/app.js") && kept_in_copies(".trash/x/app.json"));
        assert!(!kept_in_copies("target/release/x") && !kept_in_copies("web/node_modules/a.js") && !kept_in_copies(".git/HEAD"));
        assert!(!kept_in_copies("mandelbrot/Cargo.lock") && kept_in_copies("mandelbrot/Cargo.toml"));
    }

    #[test]
    fn diff_shows_changes_with_context() {
        let old = b"a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = b"a\nb\nc\nD\ne\nf\ng\nh\ni\n";
        let d = line_diff(old, new).unwrap();
        assert_eq!(d, "…\n b\n c\n-d\n+D\n e\n f\n g\n h\n+i\n");
        assert_eq!(line_diff(b"same\n", b"same\n").unwrap(), "");
        assert!(line_diff(b"\xff\xfe", b"x").is_none(), "binary is not compared");
        assert_eq!(change(None, Some(b"x")), Change::Added);
        assert_eq!(change(Some(b"x"), Some(b"x")), Change::Same);
    }
}
