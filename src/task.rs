//! A task: one markdown file, `NNN-slug.md`, living in a column directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::board::Column;

/// Longest slug we put in a filename (matches the pre-Rust `cut -c1-50`).
const MAX_SLUG: usize = 50;

#[derive(Clone, Debug)]
pub struct Task {
    pub path: PathBuf,
    pub column: Column,
    /// The `NNN` prefix, when the filename has one.
    pub id: Option<u32>,
    /// Filename without the `.md` extension, e.g. `001-first-task`.
    pub stem: String,
}

impl Task {
    /// Build a task from a path inside a column directory.
    pub fn from_path(path: &Path, column: Column) -> Option<Self> {
        if path.extension()? != "md" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?.to_string();
        let id = stem
            .split_once('-')
            .and_then(|(head, _)| head.parse::<u32>().ok());
        Some(Self {
            path: path.to_path_buf(),
            column,
            id,
            stem,
        })
    }

    pub fn file_name(&self) -> String {
        format!("{}.md", self.stem)
    }

    /// Look up a frontmatter key. Tolerant by design: task files are rendered
    /// from user-editable templates, so this reads the first `key: value` line
    /// rather than pretending to be a YAML parser.
    pub fn meta(&self, key: &str) -> Option<String> {
        let text = fs::read_to_string(&self.path).ok()?;
        meta_in(&text, key).map(str::to_string)
    }

    /// Id as displayed on the board: frontmatter first, then the filename.
    pub fn id_display(&self) -> String {
        self.meta("id")
            .filter(|id| !id.is_empty())
            .or_else(|| self.id.map(|n| format!("{n:03}")))
            .unwrap_or_else(|| "???".to_string())
    }

    /// Title as displayed on the board, falling back to the filename.
    pub fn title(&self) -> String {
        self.meta("title")
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| self.stem.clone())
    }
}

/// Search a task's frontmatter block (or, if it has none, the whole file) for
/// the first `key: value` line.
fn meta_in<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let mut lines = text.lines();
    let fenced = text.starts_with("---");
    if fenced {
        lines.next();
    }
    for line in lines {
        if fenced && line.trim_end() == "---" {
            break;
        }
        if let Some(rest) = line.strip_prefix(key)
            && let Some(value) = rest.strip_prefix(':')
        {
            return Some(unquote(value.trim()));
        }
    }
    None
}

fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// `"Publish to Marketplace"` -> `"publish-to-marketplace"`.
///
/// ASCII-only on purpose: the id and slug are a filename, and filenames that
/// survive every filesystem, shell and URL are worth more than transliteration.
pub fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut pending_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(ch.to_ascii_lowercase());
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    slug.truncate(MAX_SLUG);
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "task".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowercases_and_joins_words() {
        assert_eq!(slugify("Publish to Marketplace"), "publish-to-marketplace");
    }

    #[test]
    fn slugify_collapses_runs_and_trims_edges() {
        assert_eq!(slugify("  --Fix:  the *thing*!  "), "fix-the-thing");
    }

    #[test]
    fn slugify_truncates_without_a_trailing_dash() {
        let slug = slugify(&"word ".repeat(30));
        assert!(slug.len() <= MAX_SLUG, "{slug}");
        assert!(!slug.ends_with('-'), "{slug}");
    }

    #[test]
    fn slugify_never_returns_empty() {
        assert_eq!(slugify("***"), "task");
        assert_eq!(slugify(""), "task");
    }

    #[test]
    fn from_path_parses_the_id_prefix() {
        let task = Task::from_path(Path::new("/b/todo/007-do-it.md"), Column::Todo).unwrap();
        assert_eq!(task.id, Some(7));
        assert_eq!(task.stem, "007-do-it");
        assert_eq!(task.file_name(), "007-do-it.md");
    }

    #[test]
    fn from_path_ignores_non_markdown_and_unnumbered_files() {
        assert!(Task::from_path(Path::new("/b/todo/.gitkeep"), Column::Todo).is_none());
        let task = Task::from_path(Path::new("/b/todo/notes.md"), Column::Todo).unwrap();
        assert_eq!(task.id, None);
    }

    #[test]
    fn meta_reads_the_frontmatter_block_only() {
        let text = "---\nid: \"001\"\ntitle: \"First task\"\ntags: [x,y]\n---\n\n## Notes\n\ntitle: not this one\n";
        assert_eq!(meta_in(text, "id"), Some("001"));
        assert_eq!(meta_in(text, "title"), Some("First task"));
        assert_eq!(meta_in(text, "tags"), Some("[x,y]"));
        assert_eq!(meta_in(text, "missing"), None);
    }

    #[test]
    fn meta_falls_back_to_the_whole_file_without_frontmatter() {
        assert_eq!(meta_in("# Heading\ntitle: Bare\n", "title"), Some("Bare"));
    }

    #[test]
    fn unquote_strips_matching_quotes_only() {
        assert_eq!(unquote("\"First task\""), "First task");
        assert_eq!(unquote("'First task'"), "First task");
        assert_eq!(unquote("[x,y]"), "[x,y]");
        assert_eq!(unquote("\""), "\"");
    }
}
