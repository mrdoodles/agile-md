//! amd — a tiny filesystem Kanban in markdown (agile-md).
//!
//! Tasks are markdown files that move between `todo/`, `doing/` and `done/` in
//! `<repo-root>/tasks`. Status is the folder; `git mv` is the audit trail; the
//! ticket format is a MiniJinja template you can edit.

mod board;
mod form;
mod git;
mod task;
mod templates;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};

use board::{Board, Column};
use templates::{DEFAULT_TEMPLATE, TEMPLATE_SUFFIX, TaskContext, Templates};

const AFTER_HELP: &str = "\
A <REF> is a task id (e.g. 7 or 007) or a unique slug substring. Leave it out
in a terminal and amd asks: `amd new` fills in a form, `amd start` offers a
list of tasks to pick from.

Environment:
  AMD_DIR       board directory name under the repository root (default: tasks)
  AMD_YES       set to 1 to create a missing board without prompting
  AMD_NO_INPUT  set to 1 to never prompt (same as --no-input)
  EDITOR        editor used by `amd edit` (default: vi)

Tasks are rendered from MiniJinja templates. `amd templates` lists them and
`amd templates eject task` writes an editable copy into <board>/templates/.
A template that uses `extra.<name>` gets asked for it in the form.";

#[derive(Parser, Debug)]
#[command(
    name = "amd",
    version,
    about = "A tiny filesystem Kanban: markdown tasks in todo/, doing/ and done/",
    after_help = AFTER_HELP,
    disable_help_subcommand = false
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
    /// Never prompt; missing values are an error instead
    #[arg(long, global = true)]
    no_input: bool,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Create the board in this repository
    Init,
    /// Create a task in todo/ from a template
    New(NewArgs),
    /// Show all columns (the default command)
    Board,
    /// List a column: todo, doing, done or all
    #[command(alias = "list")]
    Ls {
        #[arg(default_value = "all", value_name = "COLUMN")]
        column: String,
    },
    /// Move a task todo -> doing
    Start {
        #[arg(value_name = "REF")]
        task: Option<String>,
    },
    /// Move a task doing -> done
    Done {
        #[arg(value_name = "REF")]
        task: Option<String>,
    },
    /// Move a task one column left
    Back {
        #[arg(value_name = "REF")]
        task: Option<String>,
    },
    /// Print a task
    Show {
        #[arg(value_name = "REF")]
        task: Option<String>,
    },
    /// Open a task in $EDITOR
    Edit {
        #[arg(value_name = "REF")]
        task: Option<String>,
    },
    /// Inspect and customise the task templates
    Templates {
        #[command(subcommand)]
        command: Option<TemplateCmd>,
    },
}

#[derive(Args, Debug)]
struct NewArgs {
    /// Task title; omit it in a terminal to fill in the form
    #[arg(value_name = "TITLE")]
    title: Option<String>,
    /// Tag to add (repeatable)
    #[arg(short = 't', long = "tag", value_name = "TAG")]
    tags: Vec<String>,
    /// Template to render
    #[arg(short = 'T', long, default_value = DEFAULT_TEMPLATE, value_name = "NAME")]
    template: String,
    /// Ask for every field, even the ones given as arguments
    #[arg(short, long)]
    interactive: bool,
    /// Extra template variable, available as extra.KEY (repeatable)
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE", value_parser = parse_key_value)]
    set: Vec<(String, String)>,
    /// Open the new task in $EDITOR
    #[arg(short, long)]
    edit: bool,
}

#[derive(Subcommand, Debug)]
enum TemplateCmd {
    /// List the available templates (the default)
    List,
    /// Print a template's source
    Show {
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Copy a built-in template into <board>/templates/ so you can edit it
    Eject {
        #[arg(default_value = DEFAULT_TEMPLATE, value_name = "NAME")]
        name: String,
        /// Overwrite an existing board template
        #[arg(short, long)]
        force: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("amd: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    form::set_no_input(cli.no_input);
    let command = cli.command.unwrap_or(Cmd::Board);

    // `init` is the one command that runs without an existing board.
    if let Cmd::Init = command {
        let board = Board::locate()?;
        board.create()?;
        println!("Initialised agile-md board at {}", board.root.display());
        return Ok(());
    }

    let board = Board::ensure()?;
    match command {
        Cmd::Init => unreachable!("handled above"),
        Cmd::New(args) => cmd_new(&board, args),
        Cmd::Board => cmd_ls(&board, "all"),
        Cmd::Ls { column } => cmd_ls(&board, &column),
        Cmd::Start { task } => {
            let task = resolve(&board, task, "start", &[Column::Todo])?;
            board.move_task(&task, Column::Doing)
        }
        Cmd::Done { task } => {
            let task = resolve(&board, task, "done", &[Column::Doing])?;
            board.move_task(&task, Column::Done)
        }
        Cmd::Back { task } => {
            let task = resolve(&board, task, "back", &[Column::Doing, Column::Done])?;
            match task.column.left() {
                Some(column) => board.move_task(&task, column),
                None => bail!("already in {}/", Column::Todo),
            }
        }
        Cmd::Show { task } => {
            let task = resolve(&board, task, "show", &Column::ALL)?;
            let body =
                fs::read(&task.path).with_context(|| format!("reading {}", task.path.display()))?;
            io::stdout().write_all(&body)?;
            Ok(())
        }
        Cmd::Edit { task } => {
            let task = resolve(&board, task, "edit", &Column::ALL)?;
            open_editor(&task.path)
        }
        Cmd::Templates { command } => cmd_templates(&board, command.unwrap_or(TemplateCmd::List)),
    }
}

/// Resolve a task reference, or offer a picker when it was left out.
fn resolve(
    board: &Board,
    reference: Option<String>,
    command: &str,
    columns: &[Column],
) -> Result<task::Task> {
    if let Some(reference) = reference {
        return board.find(&reference);
    }
    if !form::available() {
        bail!("usage: amd {command} <ref>");
    }
    let mut choices = Vec::new();
    for column in columns {
        choices.extend(board.tasks_in(*column)?);
    }
    if choices.is_empty() {
        let names: Vec<String> = columns.iter().map(|c| format!("{c}/")).collect();
        bail!("no tasks in {}", names.join(" or "));
    }
    form::select_task(&format!("Which task should amd {command}?"), choices)
}

fn cmd_new(board: &Board, args: NewArgs) -> Result<()> {
    let templates = Templates::load(board)?;
    // A bare `amd new` (or -i) fills the whole form in; otherwise only the
    // fields the chosen template needs and the arguments didn't supply.
    let full_form = args.interactive || args.title.is_none();
    if full_form && !form::available() {
        bail!("usage: amd new \"<title>\" [-t tag ...]");
    }

    let template = if full_form {
        let choices = templates.task_templates();
        if choices.len() > 1 {
            form::select("Template:", choices, &args.template)?
        } else {
            args.template.clone()
        }
    } else {
        args.template.clone()
    };

    let title = match args.title {
        Some(title) if !args.interactive => title,
        title => form::required_text("Title:", title.as_deref())?,
    };

    let tags = if full_form {
        form::tags(&args.tags.join(", "), board.tags()?)?
    } else {
        args.tags
    };

    // Fields the template itself asks for: `{{ extra.owner }}` becomes a
    // question, so a template can't reference a value nobody collected.
    let mut extra: BTreeMap<String, String> = args.set.into_iter().collect();
    let required = templates.required_extras(&template)?;
    let missing: Vec<&String> = required
        .iter()
        .filter(|key| !extra.contains_key(*key))
        .collect();
    if !missing.is_empty() {
        if !form::available() {
            let flags: Vec<String> = missing.iter().map(|key| format!("--set {key}=…")).collect();
            bail!("template '{template}' needs {}", flags.join(" "));
        }
        for key in missing {
            let answer = form::text(&format!("{}:", templates::field_label(key)), None, None)?;
            extra.insert(key.clone(), answer);
        }
    }

    let number = board.next_id()?;
    let id = format!("{number:03}");
    let slug = task::slugify(&title);
    let dir = board.dir(Column::Todo);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("{id}-{slug}.md"));
    if path.exists() {
        bail!("{} already exists", path.display());
    }

    let (created, timestamp) = templates::today();
    let context = TaskContext {
        id: id.clone(),
        number,
        title,
        slug: slug.clone(),
        tags,
        created,
        timestamp,
        column: Column::Todo.to_string(),
        author: git::config("user.name").unwrap_or_else(|| "unknown".to_string()),
        email: git::config("user.email").unwrap_or_default(),
        board: board.name(),
        template: template.clone(),
        extra,
    };

    let body = templates.render(&template, &context)?;
    fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    println!("created {}/{id}-{slug}.md", Column::Todo);
    if args.edit {
        open_editor(&path)?;
    }
    Ok(())
}

fn cmd_ls(board: &Board, column: &str) -> Result<()> {
    if column.eq_ignore_ascii_case("all") {
        for column in Column::ALL {
            list_column(board, column)?;
        }
        return Ok(());
    }
    let column = Column::parse(column)
        .with_context(|| format!("unknown column '{column}' (todo, doing, done or all)"))?;
    list_column(board, column)
}

fn list_column(board: &Board, column: Column) -> Result<()> {
    println!();
    println!("{}", column.as_str().to_uppercase());
    let tasks = board.tasks_in(column)?;
    if tasks.is_empty() {
        println!("  (empty)");
        return Ok(());
    }
    for task in tasks {
        println!("  [{}] {}", task.id_display(), task.title());
    }
    Ok(())
}

fn cmd_templates(board: &Board, command: TemplateCmd) -> Result<()> {
    let templates = Templates::load(board)?;
    match command {
        TemplateCmd::List => {
            let width = templates
                .list()
                .map(|(name, _)| name.len())
                .max()
                .unwrap_or(0);
            for (name, source) in templates.list() {
                println!("{name:width$}  {}", source.describe());
            }
            Ok(())
        }
        TemplateCmd::Show { name } => {
            print!("{}", templates.source_text(&name)?);
            Ok(())
        }
        TemplateCmd::Eject { name, force } => {
            let source = templates::builtin_source(&name)
                .with_context(|| format!("'{name}' is not a built-in template"))?;
            let dir = board.templates_dir();
            fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            let path = dir.join(format!("{name}{TEMPLATE_SUFFIX}"));
            if path.exists() && !force {
                bail!(
                    "{} already exists (use --force to overwrite)",
                    path.display()
                );
            }
            fs::write(&path, source).with_context(|| format!("writing {}", path.display()))?;
            println!("wrote {}", path.display());
            Ok(())
        }
    }
}

fn open_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR")
        .ok()
        .filter(|editor| !editor.is_empty())
        .unwrap_or_else(|| "vi".to_string());
    let status = Command::new(&editor)
        .arg(path)
        .status()
        .with_context(|| format!("running {editor}"))?;
    if !status.success() {
        bail!("{editor} exited with {status}");
    }
    Ok(())
}

fn parse_key_value(raw: &str) -> Result<(String, String), String> {
    match raw.split_once('=') {
        Some((key, value)) if !key.is_empty() => Ok((key.to_string(), value.to_string())),
        _ => Err(format!("expected KEY=VALUE, got '{raw}'")),
    }
}
