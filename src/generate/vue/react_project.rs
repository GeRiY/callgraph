//! Project knowledge for the React generator: the `.tsx` / `.jsx` side of `VueProject` (style sampling, export
//! style, component lookup). The walks and file texts are memoised like the Vue ones.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::edit::indent_unit;
use super::project::{CodeStyle, ROOTS, SAMPLE, VueProject, uses_semicolons, walk_ext};
use crate::generate::edit::split_lines;
use crate::generate::fs::MemoWalk;

/// The extensions of React component files.
const JSX_EXTS: &[&str] = &["tsx", "jsx"];

/// How a component file exports its component: `export default …` (true) or `export function X` /
/// `export const X` with a capitalised name (false); `None` without either.
fn export_kind(text: &str) -> Option<bool> {
    let mut named = false;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("export default") {
            return Some(true);
        }
        if let Some(rest) = t.strip_prefix("export ") {
            let rest = rest.trim_start();
            let rest = ["async function ", "function ", "const ", "class "]
                .iter()
                .find_map(|k| rest.strip_prefix(k))
                .unwrap_or("");
            named |= rest.chars().next().is_some_and(char::is_uppercase);
        }
    }
    named.then_some(false)
}

impl<'a> VueProject<'a> {
    fn jsx_walk(&self, dir: &Path) -> Rc<MemoWalk<'a>> {
        let mut walks = self.jsx_walks.borrow_mut();
        let fs = self.fs;
        Rc::clone(
            walks.entry(dir.to_path_buf()).or_insert_with(|| {
                Rc::new(MemoWalk::new(walk_ext(fs, dir.to_path_buf(), JSX_EXTS)))
            }),
        )
    }

    /// The `.tsx` / `.jsx` files of a project directory (at most `limit`); empty for a missing directory.
    pub fn jsx_files(&self, rel: &str, limit: usize) -> Vec<PathBuf> {
        self.jsx_walk(&self.root.join(rel))
            .iter()
            .take(limit)
            .collect()
    }

    /// The style sample: the first `SAMPLE` React files under the sample roots (walked once).
    fn jsx_sample_files(&self) -> &[PathBuf] {
        self.jsx_sample.get_or_init(|| {
            let mut files = Vec::new();
            for dir in self.sample_roots() {
                files.extend(self.jsx_walk(dir).iter().take(SAMPLE - files.len()));
                if files.len() >= SAMPLE {
                    break;
                }
            }
            files
        })
    }

    /// TypeScript and semicolons of the React files; without a React sample the project-wide guess.
    pub fn jsx_code_style(&self) -> CodeStyle {
        let files = self.jsx_sample_files();
        if files.is_empty() {
            return self.code_style();
        }
        let mut ts = self.fs.exists(&self.root.join("tsconfig.json"));
        let (mut with, mut without) = (0usize, 0usize);
        for file in files {
            ts |= file.extension().is_some_and(|e| e == "tsx");
            match self.text(file).and_then(|t| uses_semicolons(&t)) {
                Some(true) => with += 1,
                Some(false) => without += 1,
                None => {}
            }
        }
        CodeStyle {
            ts,
            semi: with >= without,
        }
    }

    /// The indent unit (2 or 4) of the first indented React file; 2 without a sample (the React default).
    pub fn jsx_indent(&self) -> usize {
        for file in self.jsx_sample_files() {
            let Some(text) = self.text(file) else {
                continue;
            };
            let (lines, _) = split_lines(&text);
            if lines.iter().any(|l| l.starts_with(' ')) {
                return indent_unit(&lines);
            }
        }
        2
    }

    /// Do the project's components use `export default` (true; also the tie / no-sample default) or named exports.
    pub fn default_exports(&self) -> bool {
        let (mut default, mut named) = (0usize, 0usize);
        for file in self.jsx_sample_files() {
            let Some(text) = self.text(file) else {
                continue;
            };
            let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if !stem.starts_with(char::is_uppercase) {
                continue;
            }
            match export_kind(&text) {
                Some(true) => default += 1,
                Some(false) => named += 1,
                None => {}
            }
        }
        default >= named
    }

    /// The first `Some` of `find` over the sampled React files' texts (the shared memo reads each file once).
    pub fn jsx_find<T>(&self, find: impl Fn(&str) -> Option<T>) -> Option<T> {
        self.jsx_sample_files()
            .iter()
            .find_map(|file| self.text(file).and_then(|text| find(&text)))
    }

    /// The import path of `<Name>.tsx` / `<Name>.jsx` (`@/Components/Card`, no extension), if the project has it.
    pub fn find_jsx_component(&self, name: &str) -> Option<String> {
        self.find_jsx_import(name).map(|(path, _)| path)
    }

    /// The import path and whether the file exports the component by name (`export function Card`, a named import)
    /// instead of `export default`.
    pub fn find_jsx_import(&self, name: &str) -> Option<(String, bool)> {
        for root in ROOTS {
            let base = self.root.join(root);
            let matches: Vec<PathBuf> = self
                .jsx_walk(&base)
                .iter()
                .filter(|p| p.file_stem().is_some_and(|n| n.to_str() == Some(name)))
                .take(20)
                .collect();
            let pick = matches
                .iter()
                .find(|p| {
                    p.to_string_lossy()
                        .to_ascii_lowercase()
                        .contains("components")
                })
                .or_else(|| matches.first());
            let Some(path) = pick else {
                continue;
            };
            let Ok(rel) = path.strip_prefix(&base) else {
                continue;
            };
            let named = self.text(path).and_then(|t| export_kind(&t)) == Some(false);
            let mut parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            if let Some(last) = parts.last_mut()
                && let Some((stem, _)) = last.rsplit_once('.')
            {
                *last = stem.to_owned();
            }
            return Some((format!("@/{}", parts.join("/")), named));
        }
        None
    }
}
