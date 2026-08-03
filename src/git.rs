//! Thin wrappers over the `git` CLI.
//!
//! agile-md deliberately shells out rather than linking a git library: the
//! behaviour is then exactly what the user would get typing the command, and
//! the binary stays small and dependency-free on the git side.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// The work-tree root of the repository containing the current directory.
pub fn repo_root() -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let root = String::from_utf8(out.stdout).ok()?;
    let root = root.trim();
    if root.is_empty() {
        None
    } else {
        Some(PathBuf::from(root))
    }
}

/// Is `file` tracked by the repository at `repo`?
pub fn is_tracked(repo: &Path, file: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "--error-unmatch"])
        .arg(file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// `git mv` — the rename is then part of the history, so `git log --follow`
/// reconstructs when a task started and finished.
pub fn mv(repo: &Path, from: &Path, to: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("mv")
        .arg(from)
        .arg(to)
        .status()
        .context("running git mv")?;
    if !status.success() {
        bail!("git mv {} {} failed", from.display(), to.display());
    }
    Ok(())
}

/// A `git config` value (used to fill `author` in task templates).
pub fn config(key: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["config", "--get", key])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8(out.stdout).ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
