//! amd — a tiny filesystem Kanban in markdown (agile-md).
//!
//! Tasks are markdown files that move between `todo/`, `doing/` and `done/` in
//! `<repo-root>/tasks`. Status is the folder; `git mv` is the audit trail; the
//! ticket format is a MiniJinja template you can edit.

mod board;
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
A <REF> is a task id (e.g. 7 or 007) or a unique slug substring.

Environment:
  AMD_DIR    board directory name under the repository root (default: tasks)
  AMD_YES    set to 1 to create a missing board without prompting
  EDITOR     editor used by `amd edit` (default: vi)

Tasks are rendered from MiniJinja templates. `amd templates` lists them and
`amd templates eject task` writes an editable copy into <board>/templates/.";

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
        task: String,
    },
    /// Move a task doing -> done
    Done {
        #[arg(value_name = "REF")]
        task: String,
    },
    /// Move a task one column left
    Back {
        #[arg(value_name = "REF")]
        task: String,
    },
    /// Print a task
    Show {
        #[arg(value_name = "REF")]
        task: String,
    },
    /// Open a task in $EDITOR
    Edit {
        #[arg(value_name = "REF")]
        task: String,
    },
    /// Inspect and customise the task templates
    Templates {
        #[command(subcommand)]
        command: Option<TemplateCmd>,
    },
}

#[derive(Args, Debug)]
struct NewArgs {
    /// Task title
    #[arg(value_name = "TITLE")]
    title: String,
    /// Tag to add (repeatable)
    #[arg(short = 't', long = "tag", value_name = "TAG")]
    tags: Vec<String>,
    /// Template to render
    #[arg(short = 'T', long, default_value = DEFAULT_TEMPLATE, value_name = "NAME")]
    template: String,
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
        Cmd::Start { task } => board.move_task(&board.find(&task)?, Column::Doing),
        Cmd::Done { task } => board.move_task(&board.find(&task)?, Column::Done),
        Cmd::Back { task } => {
            let task = board.find(&task)?;
            match task.column.left() {
                Some(column) => board.move_task(&task, column),
                None => bail!("already in {}/", Column::Todo),
            }
        }
        Cmd::Show { task } => {
            let task = board.find(&task)?;
            let body =
                fs::read(&task.path).with_context(|| format!("reading {}", task.path.display()))?;
            io::stdout().write_all(&body)?;
            Ok(())
        }
        Cmd::Edit { task } => open_editor(&board.find(&task)?.path),
        Cmd::Templates { command } => cmd_templates(&board, command.unwrap_or(TemplateCmd::List)),
    }
}

fn cmd_new(board: &Board, args: NewArgs) -> Result<()> {
    let templates = Templates::load(board)?;
    let number = board.next_id()?;
    let id = format!("{number:03}");
    let slug = task::slugify(&args.title);
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
        title: args.title,
        slug: slug.clone(),
        tags: args.tags,
        created,
        timestamp,
        column: Column::Todo.to_string(),
        author: git::config("user.name").unwrap_or_else(|| "unknown".to_string()),
        email: git::config("user.email").unwrap_or_default(),
        board: board.name(),
        template: args.template.clone(),
        extra: args.set.into_iter().collect::<BTreeMap<_, _>>(),
    };

    let body = templates.render(&args.template, &context)?;
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
