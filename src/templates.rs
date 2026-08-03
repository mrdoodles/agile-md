//! Task templates, rendered with MiniJinja.
//!
//! Every task file is produced by a template, never by string-building in the
//! CLI: that is what makes the ticket format editable (and consistent) without
//! touching the tool. The built-ins are compiled into the binary, and any
//! `<board>/templates/<name>.md.jinja` overrides or adds to them.
//!
//! Undefined variables are a hard error (`UndefinedBehavior::Strict`) so a typo
//! in a template fails loudly instead of rendering a blank line, and
//! auto-escaping is off because the output is markdown, not HTML.

use std::collections::BTreeMap;
use std::error::Error as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use minijinja::{AutoEscape, Environment, UndefinedBehavior, Value, context};
use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;

use crate::board::{Board, Column};

/// Template used by `amd new` when `--template` isn't given.
pub const DEFAULT_TEMPLATE: &str = "task";

/// Suffix for template files in `<board>/templates/`.
pub const TEMPLATE_SUFFIX: &str = ".md.jinja";

/// The template that renders the board's own README on `amd init`.
const BOARD_README: &str = "board-readme";

const BUILTINS: &[(&str, &str)] = &[
    ("task", include_str!("../templates/task.md.jinja")),
    ("bug", include_str!("../templates/bug.md.jinja")),
    (
        BOARD_README,
        include_str!("../templates/board-readme.md.jinja"),
    ),
];

/// Where a template came from — shown by `amd templates`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Builtin,
    Board(PathBuf),
}

impl Source {
    pub fn describe(&self) -> String {
        match self {
            Source::Builtin => "built-in".to_string(),
            Source::Board(path) => path.display().to_string(),
        }
    }
}

/// The variables every task template is rendered with.
#[derive(Debug, Serialize)]
pub struct TaskContext {
    /// Zero-padded id, e.g. `007`.
    pub id: String,
    /// The same id as a number, for arithmetic and comparisons.
    pub number: u32,
    pub title: String,
    pub slug: String,
    pub tags: Vec<String>,
    /// Local date, `YYYY-MM-DD`.
    pub created: String,
    /// Local timestamp, RFC 3339.
    pub timestamp: String,
    /// Column the task is created in.
    pub column: String,
    /// `git config user.name`, or `unknown`.
    pub author: String,
    /// `git config user.email`, or an empty string.
    pub email: String,
    /// Board directory name (`tasks`).
    pub board: String,
    /// Name of the template being rendered.
    pub template: String,
    /// Anything passed with `--set key=value`.
    pub extra: BTreeMap<String, String>,
}

pub struct Templates {
    env: Environment<'static>,
    sources: BTreeMap<String, Source>,
}

impl Templates {
    /// The compiled-in templates only.
    pub fn builtin() -> Result<Self> {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        // Markdown output: HTML escaping would mangle it.
        env.set_auto_escape_callback(|_| AutoEscape::None);
        env.set_keep_trailing_newline(true);
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);
        // Frontmatter safety: quote and escape a value so a title containing a
        // quote can't corrupt the block.
        env.add_filter("yaml", yaml_filter);

        let mut sources = BTreeMap::new();
        for (name, source) in BUILTINS {
            env.add_template(name, source)
                .with_context(|| format!("built-in template '{name}'"))?;
            sources.insert((*name).to_string(), Source::Builtin);
        }
        Ok(Self { env, sources })
    }

    /// Built-ins plus everything in `<board>/templates/`, which wins on a
    /// name clash.
    pub fn load(board: &Board) -> Result<Self> {
        let mut templates = Templates::builtin()?;
        let dir = board.templates_dir();
        if !dir.is_dir() {
            return Ok(templates);
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<std::io::Result<_>>()
            .with_context(|| format!("reading {}", dir.display()))?;
        entries.sort();
        for path in entries {
            let Some(name) = template_name(&path) else {
                continue;
            };
            let source =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            templates
                .env
                .add_template_owned(name.clone(), source)
                .map_err(|err| render_error(&path.display().to_string(), err))?;
            templates.sources.insert(name, Source::Board(path));
        }
        Ok(templates)
    }

    /// Template names and where each came from.
    pub fn list(&self) -> impl Iterator<Item = (&str, &Source)> {
        self.sources
            .iter()
            .map(|(name, source)| (name.as_str(), source))
    }

    /// The template's text, from the board file or the binary.
    pub fn source_text(&self, name: &str) -> Result<String> {
        match self.sources.get(name) {
            Some(Source::Board(path)) => {
                fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
            }
            Some(Source::Builtin) => Ok(builtin_source(name)
                .expect("built-in registered without source")
                .to_string()),
            None => bail!(unknown_template(name)),
        }
    }

    pub fn render<S: Serialize>(&self, name: &str, ctx: S) -> Result<String> {
        let template = self
            .env
            .get_template(name)
            .map_err(|_| anyhow!(unknown_template(name)))?;
        template.render(ctx).map_err(|err| render_error(name, err))
    }
}

/// Render the board's README (used by `amd init`).
pub fn render_board_readme(board: &Board) -> Result<String> {
    let templates = Templates::load(board)?;
    let columns: Vec<&str> = Column::ALL.iter().map(|c| c.as_str()).collect();
    templates.render(
        BOARD_README,
        context! {
            board => board.name(),
            columns => columns,
            created => today().0,
        },
    )
}

/// The source of a built-in template, if `name` is one.
pub fn builtin_source(name: &str) -> Option<&'static str> {
    BUILTINS
        .iter()
        .find(|(builtin, _)| *builtin == name)
        .map(|(_, source)| *source)
}

/// `<name>.md.jinja` (also `.jinja` / `.j2`) -> `name`; anything else is
/// ignored, so a README or a stray `.swp` in the templates dir is harmless.
pub fn template_name(path: &Path) -> Option<String> {
    let file = path.file_name()?.to_str()?;
    for suffix in [TEMPLATE_SUFFIX, ".jinja", ".j2"] {
        if let Some(stem) = file.strip_suffix(suffix) {
            let stem = stem.strip_suffix(".md").unwrap_or(stem);
            if stem.is_empty() {
                return None;
            }
            return Some(stem.to_string());
        }
    }
    None
}

/// Local date (`YYYY-MM-DD`) and RFC 3339 timestamp, falling back to UTC when
/// the platform won't give us an offset.
pub fn today() -> (String, String) {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let date = now
        .format(format_description!("[year]-[month]-[day]"))
        .unwrap_or_default();
    let timestamp = now.format(&Rfc3339).unwrap_or_default();
    (date, timestamp)
}

fn yaml_filter(value: Value) -> String {
    let raw = value.to_string();
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn unknown_template(name: &str) -> String {
    format!("no template '{name}' (try: amd templates)")
}

/// MiniJinja errors carry the template, line and a cause chain — flatten all of
/// it into the message, since only the top line survives otherwise.
fn render_error(name: &str, err: minijinja::Error) -> anyhow::Error {
    let mut message = format!("template '{name}': {err}");
    if let Some(line) = err.line() {
        message.push_str(&format!(" (line {line})"));
    }
    let mut cause = err.source();
    while let Some(err) = cause {
        message.push_str(&format!("\n  caused by: {err}"));
        cause = err.source();
    }
    let debug_info = err.display_debug_info().to_string();
    if !debug_info.trim().is_empty() {
        message.push('\n');
        message.push_str(debug_info.trim_end());
    }
    anyhow!(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> TaskContext {
        TaskContext {
            id: "001".to_string(),
            number: 1,
            title: "First task".to_string(),
            slug: "first-task".to_string(),
            tags: vec!["x".to_string(), "y".to_string()],
            created: "2026-08-03".to_string(),
            timestamp: "2026-08-03T09:00:00+01:00".to_string(),
            column: "todo".to_string(),
            author: "t".to_string(),
            email: "t@t.co".to_string(),
            board: "tasks".to_string(),
            template: "task".to_string(),
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn task_template_renders_the_documented_frontmatter() {
        let rendered = Templates::builtin().unwrap().render("task", ctx()).unwrap();
        assert!(rendered.starts_with("---\n"), "{rendered}");
        assert!(rendered.contains("\nid: \"001\"\n"), "{rendered}");
        assert!(rendered.contains("\ntitle: \"First task\"\n"), "{rendered}");
        assert!(rendered.contains("\ntags: [x,y]\n"), "{rendered}");
        assert!(rendered.contains("## Checklist"), "{rendered}");
        assert!(rendered.ends_with('\n'), "{rendered}");
    }

    #[test]
    fn every_builtin_renders_with_the_standard_context() {
        let templates = Templates::builtin().unwrap();
        for (name, _) in BUILTINS {
            if *name == BOARD_README {
                continue;
            }
            templates
                .render(name, ctx())
                .unwrap_or_else(|err| panic!("{name}: {err:#}"));
        }
    }

    #[test]
    fn a_title_with_quotes_cannot_break_the_frontmatter() {
        let mut c = ctx();
        c.title = r#"Fix the "quoted" \ thing"#.to_string();
        let rendered = Templates::builtin().unwrap().render("task", c).unwrap();
        assert!(
            rendered.contains(r#"title: "Fix the \"quoted\" \\ thing""#),
            "{rendered}"
        );
    }

    #[test]
    fn undefined_variables_are_an_error_not_a_blank() {
        let mut templates = Templates::builtin().unwrap();
        templates
            .env
            .add_template_owned("typo".to_string(), "{{ titel }}".to_string())
            .unwrap();
        let err = templates.render("typo", ctx()).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("undefined value"), "{message}");
        assert!(message.contains("line 1"), "{message}");
    }

    #[test]
    fn extra_variables_are_available_to_templates() {
        let mut templates = Templates::builtin().unwrap();
        templates
            .env
            .add_template_owned("extra".to_string(), "owner: {{ extra.owner }}".to_string())
            .unwrap();
        let mut c = ctx();
        c.extra.insert("owner".to_string(), "tim".to_string());
        assert_eq!(templates.render("extra", c).unwrap(), "owner: tim");
    }

    #[test]
    fn unknown_templates_point_at_the_templates_command() {
        let err = Templates::builtin()
            .unwrap()
            .render("nope", ctx())
            .unwrap_err();
        assert!(format!("{err:#}").contains("amd templates"), "{err:#}");
    }

    #[test]
    fn template_names_come_from_the_filename() {
        assert_eq!(
            template_name(Path::new("/b/templates/story.md.jinja")).as_deref(),
            Some("story")
        );
        assert_eq!(
            template_name(Path::new("/b/templates/story.j2")).as_deref(),
            Some("story")
        );
        assert_eq!(template_name(Path::new("/b/templates/README.md")), None);
    }
}
