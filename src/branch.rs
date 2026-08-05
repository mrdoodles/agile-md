//! Ticket types, labels, and the branch names they produce.
//!
//! There are two kinds of ticket — a **development** ticket and an **admin**
//! ticket — and each is a template. Development work gets a branch; admin work
//! (a rota, a renewal, an approval) has nothing to check out, so it doesn't.
//! The rule lives in the template itself: a ticket whose template records a
//! `branch` gets one, which means a custom template opts in by using it.
//!
//! On top of that every ticket carries labels:
//!
//! - `type` — a conventional-commit type (`feat`, `fix`, `docs`, …) on
//!   development tickets. It decides the branch prefix.
//! - `epic` — optional, groups tasks across a body of work.
//! - `story` — optional, groups tasks within an epic.
//!
//! The type is a *commit* type, but branches follow the *branch* convention
//! (`feature/`, `bugfix/`, `hotfix/`, `release/`, `chore/`) — the same split
//! mrdoodles/conventional-validator enforces. `prefix()` maps between them, so
//! a `feat` ticket titled "Add login" starts work on `feature/add-login`: the
//! commits and the branch both validate.

use std::env;

use anyhow::{Result, bail};

use crate::task;

/// Conventional-commit types, the accepted values of the `type` label.
/// Override the list with `AMD_TYPES="feat,fix,docs"`.
pub const DEFAULT_TYPES: [&str; 11] = [
    "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert",
];

/// Commit type -> branch prefix. Anything unmapped becomes `chore`, which is
/// what the branch convention has for "everything else".
const PREFIXES: [(&str, &str); 3] = [("feat", "feature"), ("fix", "bugfix"), ("revert", "hotfix")];

/// Fallback branch prefix for types with no specific mapping.
const DEFAULT_PREFIX: &str = "chore";

/// The one type an admin ticket carries. It isn't a conventional-commit type —
/// it's the answer to "this isn't code", which is why it lives alongside them
/// in a single list rather than in a second field.
pub const ADMIN: &str = "admin";

/// Everything a ticket's `type` may be: `admin` plus the change types. One
/// question instead of two — the ticket type and the change it makes were
/// always the same decision.
pub fn ticket_types() -> Vec<String> {
    let mut all = vec![ADMIN.to_string()];
    all.extend(types());
    all
}

/// Split a chosen type into the template that renders it and the change type
/// that names its branch. `admin` has no change type and so gets no branch.
pub fn resolve(chosen: &str) -> (String, String) {
    if chosen == ADMIN {
        (ADMIN.to_string(), String::new())
    } else {
        (
            crate::templates::DEFAULT_TEMPLATE.to_string(),
            chosen.to_string(),
        )
    }
}

/// Check a chosen type against the accepted list.
pub fn validate_ticket_type(chosen: &str) -> Result<()> {
    let types = ticket_types();
    if types.iter().any(|known| known == chosen) {
        return Ok(());
    }
    bail!(
        "unknown type '{chosen}' (expected one of: {}; set AMD_TYPES to change the list)",
        types.join(", ")
    )
}

/// The type labels this board accepts.
pub fn types() -> Vec<String> {
    match env::var("AMD_TYPES") {
        Ok(raw) if !raw.trim().is_empty() => {
            let types: Vec<String> = raw
                .split(',')
                .map(|kind| kind.trim().to_string())
                .filter(|kind| !kind.is_empty())
                .collect();
            if types.is_empty() {
                default_types()
            } else {
                types
            }
        }
        _ => default_types(),
    }
}

fn default_types() -> Vec<String> {
    DEFAULT_TYPES.iter().map(|kind| kind.to_string()).collect()
}

/// The type used when `--type` isn't given: the first of the list.
pub fn default_type() -> String {
    types()
        .first()
        .cloned()
        .unwrap_or_else(|| DEFAULT_TYPES[0].to_string())
}

/// The branch prefix a type label works on.
pub fn prefix(kind: &str) -> String {
    PREFIXES
        .iter()
        .find(|(commit_type, _)| *commit_type == kind)
        .map(|(_, prefix)| (*prefix).to_string())
        // A custom AMD_TYPES entry that is already a branch prefix keeps its
        // own name rather than collapsing to chore.
        .or_else(|| {
            ["feature", "bugfix", "hotfix", "release", "chore"]
                .iter()
                .find(|prefix| **prefix == kind)
                .map(|prefix| (*prefix).to_string())
        })
        .unwrap_or_else(|| DEFAULT_PREFIX.to_string())
}

/// Check a type label against the accepted list.
pub fn validate_type(kind: &str) -> Result<()> {
    let types = types();
    if types.iter().any(|known| known == kind) {
        return Ok(());
    }
    bail!(
        "unknown type '{kind}' (expected one of: {}; set AMD_TYPES to change the list)",
        types.join(", ")
    )
}

/// `("feat", "Add login")` -> `feature/add-login`.
pub fn for_title(kind: &str, title: &str) -> Result<String> {
    let slug = task::slugify(title);
    if slug.is_empty() {
        bail!(
            "title '{title}' has no letters or numbers to build a branch name from \
             (it becomes the branch {}/<title>)",
            prefix(kind)
        );
    }
    let name = format!("{}/{slug}", prefix(kind));
    validate(&name)?;
    Ok(name)
}

/// Reject anything `git check-ref-format --branch` would.
///
/// Worth doing properly: a branch name can also arrive from a ticket's
/// `branch:` frontmatter or from `--branch`, which are arbitrary text.
pub fn validate(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("branch name is empty");
    }
    if name == "@" {
        bail!("'@' is not a valid branch name");
    }
    if name.starts_with('-') {
        bail!("branch name '{name}' cannot start with '-'");
    }
    if name.starts_with('/') || name.ends_with('/') {
        bail!("branch name '{name}' cannot start or end with '/'");
    }
    if name.starts_with('.') || name.ends_with('.') {
        bail!("branch name '{name}' cannot start or end with '.'");
    }
    if name.ends_with(".lock") {
        bail!("branch name '{name}' cannot end with '.lock'");
    }
    for forbidden in ["..", "//", "@{", "\\"] {
        if name.contains(forbidden) {
            bail!("branch name '{name}' cannot contain '{forbidden}'");
        }
    }
    for ch in name.chars() {
        if ch.is_control() || ch.is_whitespace() {
            bail!("branch name '{name}' cannot contain whitespace or control characters");
        }
        if "~^:?*[".contains(ch) {
            bail!("branch name '{name}' cannot contain '{ch}'");
        }
    }
    Ok(())
}

/// Check a title early, at the prompt: it has to make a filename, and for a
/// development ticket that filename slug is also the branch. Catching it here
/// beats leaving a ticket that can never start.
pub fn validate_sluggable(title: &str) -> Result<()> {
    if title.trim().is_empty() {
        bail!("a title is required");
    }
    if task::slugify(title).is_empty() {
        bail!("title '{title}' has no letters or numbers to build a filename from");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_types_map_onto_branch_types() {
        assert_eq!(prefix("feat"), "feature");
        assert_eq!(prefix("fix"), "bugfix");
        assert_eq!(prefix("revert"), "hotfix");
        // Everything else is a chore branch.
        for kind in ["docs", "style", "refactor", "perf", "test", "build", "ci"] {
            assert_eq!(prefix(kind), "chore", "{kind}");
        }
        // A custom type that is already a branch prefix keeps its own name.
        assert_eq!(prefix("release"), "release");
    }

    #[test]
    fn a_title_becomes_a_conventional_branch() {
        assert_eq!(for_title("feat", "Add login").unwrap(), "feature/add-login");
        assert_eq!(
            for_title("fix", "Crash on SAVE!").unwrap(),
            "bugfix/crash-on-save"
        );
        assert_eq!(
            for_title("docs", "Update README").unwrap(),
            "chore/update-readme"
        );
    }

    #[test]
    fn a_title_with_nothing_sluggable_is_rejected() {
        let err = for_title("feat", "***").unwrap_err().to_string();
        assert!(err.contains("no letters or numbers"), "{err}");
    }

    #[test]
    fn git_ref_rules_are_enforced() {
        for bad in [
            "",
            "@",
            "-lead",
            "/lead",
            "trail/",
            ".lead",
            "trail.",
            "branch.lock",
            "a..b",
            "a//b",
            "a@{b",
            "a\\b",
            "a b",
            "a~b",
            "a^b",
            "a:b",
            "a?b",
            "a*b",
            "a[b",
        ] {
            assert!(validate(bad).is_err(), "{bad:?} should be rejected");
        }
        for good in ["feature/add-login", "hotfix/001-fix", "chore/deps"] {
            assert!(validate(good).is_ok(), "{good:?} should be accepted");
        }
    }

    #[test]
    fn titles_for_admin_tickets_only_need_a_filename() {
        assert!(validate_sluggable("Update the rota").is_ok());
        assert!(validate_sluggable("***").is_err());
        assert!(validate_sluggable("  ").is_err());
    }

    #[test]
    fn the_types_are_the_conventional_commit_ones() {
        assert_eq!(types(), DEFAULT_TYPES);
        assert_eq!(default_type(), "feat");
        assert!(validate_type("refactor").is_ok());
        let err = validate_type("feature").unwrap_err().to_string();
        assert!(err.contains("unknown type 'feature'"), "{err}");
        assert!(err.contains("AMD_TYPES"), "{err}");
    }
}
