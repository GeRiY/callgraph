//! The input of `impact`: a unified diff (from `git diff`, or from stdin) parsed into the changed new-side line
//! ranges per file. `git` runs as a subprocess in the project root; nothing else of git is used.

use std::io::Read;
use std::path::Path;
use std::process::Command;

use crate::error::{CallgraphError, Result};

/// Where the diff comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    /// Working tree + index vs `HEAD` (`git diff HEAD`), plus the untracked files as new files.
    WorkingTree,
    /// Index vs `HEAD` (`git diff --cached`).
    Staged,
    /// `git diff <range>`: `main...HEAD`, `HEAD~3`, a commit.
    Range(String),
    /// A unified diff read from stdin (`--diff -`).
    Stdin,
}

/// What happened to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Modified,
    Added,
    Deleted,
    Renamed,
}

/// One changed file of the diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileDiff {
    /// The new path, `/`-separated, relative to the diff root.
    pub path: String,
    /// The old path of a renamed file.
    pub old_path: Option<String>,
    pub status: Status,
    /// A binary file: no line ranges.
    pub binary: bool,
    /// New-side line ranges (inclusive) of the added and modified lines.
    pub added: Vec<(u32, u32)>,
    /// New-side lines after which lines were only removed (0 = before the first line).
    pub removed_after: Vec<u32>,
}

impl FileDiff {
    fn new(path: String, old_path: Option<String>) -> Self {
        Self {
            path,
            old_path,
            status: Status::Modified,
            binary: false,
            added: Vec::new(),
            removed_after: Vec::new(),
        }
    }

    /// A whole new file of `lines` lines (an untracked file).
    pub(crate) fn whole_file(path: String, lines: u32) -> Self {
        let mut file = Self::new(path, None);
        file.status = Status::Added;
        if lines > 0 {
            file.added.push((1, lines));
        }
        file
    }

    fn add_line(&mut self, line: u32) {
        match self.added.last_mut() {
            Some(last) if last.1 + 1 == line => last.1 = line,
            Some(last) if last.1 >= line => {}
            _ => self.added.push((line, line)),
        }
    }
}

/// The state inside one `@@` hunk: the line counters of the header and the removal waiting for a replacement.
#[derive(Default)]
struct Hunk {
    old_left: u32,
    new_left: u32,
    /// The new-side number of the next new / context line.
    new_line: u32,
    /// Lines were removed after this new-side line, and no added line has followed them yet.
    pending: Option<u32>,
}

impl Hunk {
    fn active(&self) -> bool {
        self.old_left > 0 || self.new_left > 0
    }

    fn start(old_count: u32, new_start: u32, new_count: u32) -> Self {
        Self {
            old_left: old_count,
            new_left: new_count,
            // a hunk without new lines names the line before the change
            new_line: if new_count == 0 {
                new_start + 1
            } else {
                new_start
            },
            pending: None,
        }
    }

    fn flush(&mut self, file: &mut FileDiff) {
        if let Some(after) = self.pending.take() {
            file.removed_after.push(after);
        }
    }

    fn feed(&mut self, line: &str, file: &mut FileDiff) {
        match line.as_bytes().first() {
            // "\ No newline at end of file"
            Some(b'\\') => return,
            Some(b'+') => {
                self.new_left = self.new_left.saturating_sub(1);
                // removed lines followed by added ones are a modification, not a removal
                self.pending = None;
                file.add_line(self.new_line);
                self.new_line += 1;
            }
            Some(b'-') => {
                self.old_left = self.old_left.saturating_sub(1);
                if self.pending.is_none() {
                    self.pending = Some(self.new_line.saturating_sub(1));
                }
            }
            _ => {
                // context line (an empty one is a context line whose trailing space was stripped)
                self.old_left = self.old_left.saturating_sub(1);
                self.new_left = self.new_left.saturating_sub(1);
                self.flush(file);
                self.new_line += 1;
            }
        }
        if !self.active() {
            self.flush(file);
        }
    }
}

/// `@@ -a[,b] +c[,d] @@` → (old start, old count, new start, new count); a missing count is 1.
fn parse_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@").or_else(|| rest.split_once(" @"))?;
    let pair = |text: &str| -> Option<(u32, u32)> {
        match text.split_once(',') {
            Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
            None => Some((text.parse().ok()?, 1)),
        }
    };
    let (old_start, old_count) = pair(old)?;
    let (new_start, new_count) = pair(new)?;
    Some((old_start, old_count, new_start, new_count))
}

/// A path of a `---` / `+++` / `diff --git` line: no timestamp, no quotes, no `a/` / `b/` prefix, `/`-separated.
fn clean_path(text: &str, prefix: &str) -> String {
    let text = text.split('\t').next().unwrap_or(text).trim();
    let text = text.trim_matches('"');
    let text = text.strip_prefix(prefix).unwrap_or(text);
    let text = text.strip_prefix("./").unwrap_or(text);
    text.replace('\\', "/")
}

/// The two paths of `diff --git a/<old> b/<new>`.
fn git_paths(rest: &str) -> (String, String) {
    let rest = rest.trim();
    match rest.rfind(" b/").or_else(|| rest.rfind(" \"b/")) {
        Some(at) => (
            clean_path(&rest[..at], "a/"),
            clean_path(&rest[at + 1..], "b/"),
        ),
        None => match rest.split_once(' ') {
            Some((old, new)) => (clean_path(old, "a/"), clean_path(new, "b/")),
            None => (clean_path(rest, "a/"), clean_path(rest, "a/")),
        },
    }
}

/// Parses a unified diff (`git diff`, `diff -u`) into the changed files. Binary files are kept with `binary: true`.
pub(crate) fn parse(text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut current: Option<FileDiff> = None;
    // the current file already had its `---` line (the next `---` starts a new file in a plain diff)
    let mut seen_old = false;
    let mut hunk = Hunk::default();
    for raw in text.lines() {
        if hunk.active()
            && let Some(file) = current.as_mut()
        {
            hunk.feed(raw, file);
            continue;
        }
        if let Some(rest) = raw.strip_prefix("diff --git ") {
            files.extend(current.take());
            let (old, new) = git_paths(rest);
            let old_path = (old != new).then_some(old);
            current = Some(FileDiff::new(new, old_path));
            seen_old = false;
            hunk = Hunk::default();
        } else if let Some(rest) = raw.strip_prefix("--- ") {
            if current.is_none() || seen_old {
                files.extend(current.take());
                current = Some(FileDiff::new(String::new(), None));
            }
            seen_old = true;
            if let Some(file) = current.as_mut() {
                let path = clean_path(rest, "a/");
                if path == "/dev/null" {
                    file.status = Status::Added;
                } else if file.path.is_empty() {
                    file.path = path;
                }
            }
        } else if let Some(rest) = raw.strip_prefix("+++ ") {
            if let Some(file) = current.as_mut() {
                let path = clean_path(rest, "b/");
                if path == "/dev/null" {
                    file.status = Status::Deleted;
                } else {
                    file.path = path;
                }
            }
        } else if raw.starts_with("@@ ") {
            if let (Some((_, old_count, new_start, new_count)), Some(_)) =
                (parse_header(raw), current.as_ref())
            {
                hunk = Hunk::start(old_count, new_start, new_count);
            }
        } else if let Some(file) = current.as_mut() {
            if raw.starts_with("new file mode") {
                file.status = Status::Added;
            } else if raw.starts_with("deleted file mode") {
                file.status = Status::Deleted;
            } else if let Some(from) = raw.strip_prefix("rename from ") {
                file.old_path = Some(clean_path(from, ""));
                file.status = Status::Renamed;
            } else if let Some(to) = raw.strip_prefix("rename to ") {
                file.path = clean_path(to, "");
                file.status = Status::Renamed;
            } else if raw.starts_with("Binary files ") || raw.starts_with("GIT binary patch") {
                file.binary = true;
            }
        }
    }
    files.extend(current);
    files.retain(|file| !file.path.is_empty());
    merge_same_path(files)
}

/// Concatenated diffs may name a file twice: one entry per path, the ranges joined.
fn merge_same_path(files: Vec<FileDiff>) -> Vec<FileDiff> {
    let mut merged: Vec<FileDiff> = Vec::new();
    for file in files {
        match merged.iter_mut().find(|seen| seen.path == file.path) {
            Some(seen) => {
                seen.added.extend(file.added);
                seen.added.sort_unstable();
                seen.removed_after.extend(file.removed_after);
                seen.removed_after.sort_unstable();
                seen.binary |= file.binary;
            }
            None => merged.push(file),
        }
    }
    merged
}

/// The diff text of the source, plus (working tree only) the untracked files.
pub(crate) struct Loaded {
    pub diff: String,
    pub untracked: Vec<String>,
}

/// Runs `git` in `root`; a missing git or a failed run is a one-line usage error (exit code 2).
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .map_err(|error| {
            CallgraphError::Usage(if error.kind() == std::io::ErrorKind::NotFound {
                crate::t!(
                    "impact: git was not found (install it, or pipe a unified diff: --diff -)",
                    "impact: a git nem található (telepítsd, vagy add át a diffet: --diff -)"
                )
                .to_owned()
            } else {
                crate::tf!(
                    "impact: cannot run git: {error}",
                    "impact: a git nem futtatható: {error}"
                )
            })
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim();
        let first = first.strip_prefix("fatal: ").unwrap_or(first);
        let shown = root.display();
        return Err(CallgraphError::Usage(crate::tf!(
            "impact: git diff failed in {shown}: {first}",
            "impact: a git diff nem sikerült itt: {shown}: {first}"
        )));
    }
    Ok(output.stdout)
}

/// Loads the diff of `source`; `stdin` is read only for `DiffSource::Stdin`.
pub(crate) fn load(root: &Path, source: &DiffSource, stdin: &mut dyn Read) -> Result<Loaded> {
    let mut args = vec!["diff", "-U0", "--no-color", "--no-ext-diff", "--relative"];
    match source {
        DiffSource::Stdin => {
            let mut text = String::new();
            stdin.read_to_string(&mut text)?;
            return Ok(Loaded {
                diff: text,
                untracked: Vec::new(),
            });
        }
        DiffSource::WorkingTree => args.push("HEAD"),
        DiffSource::Staged => args.push("--cached"),
        DiffSource::Range(range) => {
            // an option-looking range would be read by git as a switch
            if range.trim().is_empty() || range.starts_with('-') || range.contains('\0') {
                return Err(CallgraphError::Usage(crate::tf!(
                    "impact: not a revision range: {range}",
                    "impact: nem revízió-tartomány: {range}"
                )));
            }
            args.push(range.as_str());
        }
    }
    args.push("--");
    let diff = String::from_utf8_lossy(&git(root, &args)?).into_owned();
    let untracked = if *source == DiffSource::WorkingTree {
        // a failure here only leaves the new files out
        git(root, &["ls-files", "--others", "--exclude-standard", "-z"])
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .split('\0')
                    .filter(|path| !path.is_empty())
                    .map(|path| path.replace('\\', "/"))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    Ok(Loaded { diff, untracked })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modified_lines_and_added_lines_become_new_side_ranges() {
        let diff = "diff --git a/src/A.php b/src/A.php\nindex 1..2 100644\n--- a/src/A.php\n+++ b/src/A.php\n@@ -5 +5 @@ class A\n-old\n+new\n@@ -10,0 +11,3 @@\n+a\n+b\n+c\n";
        let files = parse(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/A.php");
        assert_eq!(files[0].status, Status::Modified);
        assert_eq!(files[0].added, vec![(5, 5), (11, 13)]);
        assert!(files[0].removed_after.is_empty());
    }

    #[test]
    fn a_pure_deletion_names_the_line_before_it() {
        let diff =
            "diff --git a/x.py b/x.py\n--- a/x.py\n+++ b/x.py\n@@ -7,2 +6,0 @@ def f():\n-a\n-b\n";
        let files = parse(diff);
        assert_eq!(files[0].added, vec![]);
        assert_eq!(files[0].removed_after, vec![6]);
        // a deletion at the very top
        let top = "diff --git a/x.py b/x.py\n--- a/x.py\n+++ b/x.py\n@@ -1 +0,0 @@\n-a\n";
        assert_eq!(parse(top)[0].removed_after, vec![0]);
    }

    #[test]
    fn context_lines_do_not_count_as_changes() {
        let diff = "--- a/m.rs\n+++ b/m.rs\n@@ -2,5 +2,5 @@\n c1\n c2\n-old\n+new\n c3\n c4\n";
        let files = parse(diff);
        assert_eq!(files[0].path, "m.rs");
        assert_eq!(files[0].added, vec![(4, 4)]);
        assert!(files[0].removed_after.is_empty());
    }

    #[test]
    fn renames_new_and_deleted_files_and_binary_files() {
        let diff = "diff --git a/old/Name.php b/new/Name.php\nsimilarity index 90%\nrename from old/Name.php\nrename to new/Name.php\n@@ -3 +3 @@\n-a\n+b\ndiff --git a/n.txt b/n.txt\nnew file mode 100644\n--- /dev/null\n+++ b/n.txt\n@@ -0,0 +1,2 @@\n+x\n+y\ndiff --git a/gone.php b/gone.php\ndeleted file mode 100644\n--- a/gone.php\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-x\n-y\ndiff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n";
        let files = parse(diff);
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].path, "new/Name.php");
        assert_eq!(files[0].old_path.as_deref(), Some("old/Name.php"));
        assert_eq!(files[0].status, Status::Renamed);
        assert_eq!(files[0].added, vec![(3, 3)]);
        assert_eq!(files[1].status, Status::Added);
        assert_eq!(files[1].added, vec![(1, 2)]);
        assert_eq!(files[2].path, "gone.php");
        assert_eq!(files[2].status, Status::Deleted);
        assert!(files[3].binary);
        assert_eq!(files[3].path, "img.png");
        assert!(files[3].added.is_empty());
    }

    #[test]
    fn a_removed_line_that_looks_like_a_file_header_stays_in_its_hunk() {
        // the removed line `-- a/x` reads `--- a/x` with its marker
        let diff = "--- a/q.sql\n+++ b/q.sql\n@@ -1,2 +1,2 @@\n--- a/x\n+-- b/y\n keep\n";
        let files = parse(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].added, vec![(1, 1)]);
    }

    #[test]
    fn plain_unified_diffs_and_no_newline_markers_parse() {
        let diff = "--- a/one.js\t2020-01-01\n+++ b/one.js\t2020-01-02\n@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n\\ No newline at end of file\n--- a/two.js\n+++ b/two.js\n@@ -2,0 +3 @@\n+z\n";
        let files = parse(diff);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "one.js");
        assert_eq!(files[0].added, vec![(1, 1)]);
        assert_eq!(files[1].path, "two.js");
        assert_eq!(files[1].added, vec![(3, 3)]);
    }

    #[test]
    fn header_counts_default_to_one() {
        assert_eq!(parse_header("@@ -3 +4 @@"), Some((3, 1, 4, 1)));
        assert_eq!(parse_header("@@ -3,0 +4,2 @@ fn x"), Some((3, 0, 4, 2)));
        assert_eq!(parse_header("@@@ -1 -1 +1 @@@"), None);
    }

    #[test]
    fn a_range_that_looks_like_an_option_is_refused() {
        let root = std::env::temp_dir();
        let mut empty = std::io::empty();
        let error = load(&root, &DiffSource::Range("--output=x".into()), &mut empty).err();
        assert!(matches!(error, Some(CallgraphError::Usage(_))));
    }

    #[test]
    fn the_same_file_twice_is_one_entry() {
        let one = "diff --git a/a.php b/a.php\n--- a/a.php\n+++ b/a.php\n@@ -9 +9 @@\n-x\n+y\n";
        let two = "diff --git a/a.php b/a.php\n--- a/a.php\n+++ b/a.php\n@@ -3 +3 @@\n-x\n+y\n";
        let files = parse(&format!("{one}{two}"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].added, vec![(3, 3), (9, 9)]);
    }

    #[test]
    fn a_whole_new_file_is_one_range() {
        let file = FileDiff::whole_file("a.php".into(), 12);
        assert_eq!(file.added, vec![(1, 12)]);
        assert_eq!(file.status, Status::Added);
        assert!(FileDiff::whole_file("e.php".into(), 0).added.is_empty());
    }
}
