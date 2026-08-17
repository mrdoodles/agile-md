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

/// The checked-out branch, or `None` on a detached HEAD.
pub fn current_branch(repo: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["branch", "--show-current"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let branch = String::from_utf8(out.stdout).ok()?;
    let branch = branch.trim();
    if branch.is_empty() {
        None
    } else {
        Some(branch.to_string())
    }
}

pub fn branch_exists(repo: &Path, name: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["show-ref", "--verify", "--quiet"])
        .arg(format!("refs/heads/{name}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Switch to `name`, creating it from the current HEAD when `create` is set.
/// Any staged changes (such as the `git mv` of the task itself) come along.
pub fn switch_branch(repo: &Path, name: &str, create: bool) -> Result<()> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).arg("switch");
    if create {
        command.arg("-c");
    }
    let status = command
        .arg(name)
        .status()
        .with_context(|| format!("running git switch {name}"))?;
    if !status.success() {
        bail!("git switch {name} failed");
    }
    Ok(())
}

/// Drop a file from the index while leaving it on disk. Used when a ticket is
/// archived: the archive is gitignored, so `git mv` would refuse, and the
/// history should record the ticket leaving the board.
pub fn untrack(repo: &Path, file: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rm", "--cached", "--quiet", "--"])
        .arg(file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("running git rm --cached")?;
    if !status.success() {
        bail!("git rm --cached {} failed", file.display());
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
