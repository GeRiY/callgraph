//! A gyökér git-ága a `root:` sorhoz (`root: /x/y (branch: feature/z)`): így az agent rögtön látja, ha nem abban a
//! munkapéldányban dolgozik, ahol az ágat váltotta (pl. a `--project` alias egy másik klónra vagy worktree-re mutat).
//! Csak fájlolvasás (`.git/HEAD`, worktree-nél a `gitdir:` célja), git-parancs nincs; ha nincs git: üres.

use std::path::Path;

/// A `.git` keresésének felső korlátja (a gyökértől felfelé).
const MAX_UP: usize = 32;

/// Az ág neve (`feature/z`), leválasztott HEAD-nél `detached <sha7>`; nincs git-tároló: `None`.
pub fn branch_of(root: &Path) -> Option<String> {
    let mut dir = Some(root);
    for _ in 0..MAX_UP {
        let current = dir?;
        let git = current.join(".git");
        if let Some(head) = head_file(&git) {
            return parse_head(&std::fs::read_to_string(head).ok()?);
        }
        dir = current.parent();
    }
    None
}

/// A HEAD fájl: `.git/HEAD`, vagy worktree / submodule esetén a `.git` fájl `gitdir: <út>` céljában.
fn head_file(git: &Path) -> Option<std::path::PathBuf> {
    if git.is_dir() {
        let head = git.join("HEAD");
        return head.is_file().then_some(head);
    }
    let text = std::fs::read_to_string(git).ok()?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    let target = Path::new(target);
    let dir = if target.is_absolute() {
        target.to_path_buf()
    } else {
        git.parent()?.join(target)
    };
    let head = dir.join("HEAD");
    head.is_file().then_some(head)
}

/// `ref: refs/heads/x` → `x`; nyers sha → `detached <sha7>`.
fn parse_head(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(reference) = text.strip_prefix("ref:") {
        let reference = reference.trim();
        return Some(
            reference
                .strip_prefix("refs/heads/")
                .unwrap_or(reference)
                .to_owned(),
        );
    }
    let sha: String = text.chars().take(7).collect();
    (sha.len() == 7 && sha.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("detached {sha}"))
}

/// A `root:` sor vége: ` (branch: x)` / ` (ág: x)`; git nélkül üres.
pub fn root_suffix(root: &Path) -> String {
    branch_of(root).map_or_else(String::new, |branch| {
        crate::tf!(" (branch: {branch})", " (ág: {branch})")
    })
}

/// A `root:` sor útja az ág-jelölés nélkül (a sort visszaolvasó kód ezt használja).
pub fn strip_root_suffix(line: &str) -> &str {
    let line = line.trim_end();
    if !line.ends_with(')') {
        return line;
    }
    for marker in [" (branch: ", " (ág: "] {
        if let Some(at) = line.rfind(marker) {
            return &line[..at];
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_text_gives_the_branch_or_a_detached_sha() {
        assert_eq!(
            parse_head("ref: refs/heads/feature/z\n").as_deref(),
            Some("feature/z")
        );
        assert_eq!(
            parse_head("3c50a5af8012552f4395e648f614a29818ef8b60\n").as_deref(),
            Some("detached 3c50a5a")
        );
        assert_eq!(parse_head("garbage"), None);
    }

    /// Ideiglenes mappa (a teszt végén törlődik).
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new() -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("callgraph-branch-{nanos}-{}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_branch_is_found_from_a_subfolder_and_through_a_worktree_gitdir() {
        let dir = Scratch::new();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src/app")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(branch_of(&repo.join("src/app")).as_deref(), Some("main"));
        // worktree: a `.git` egy fájl, ami a valódi git-mappára mutat
        let worktree = dir.path().join("wt");
        let admin = repo.join(".git/worktrees/wt");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(admin.join("HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", admin.display()),
        )
        .unwrap();
        assert_eq!(branch_of(&worktree).as_deref(), Some("feature/x"));
    }

    #[test]
    fn the_suffix_is_stripped_when_the_root_line_is_read_back() {
        assert_eq!(strip_root_suffix("/p (branch: feature/z)"), "/p");
        assert_eq!(strip_root_suffix("/p (ág: main)"), "/p");
        assert_eq!(strip_root_suffix("/p"), "/p");
        assert_eq!(strip_root_suffix("/p (x)"), "/p (x)");
    }
}
