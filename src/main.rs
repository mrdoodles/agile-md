//! amd — a tiny filesystem Kanban in markdown (agile-md).
//!
//! Tasks are markdown files that move between `todo/`, `doing/` and `done/` in
//! `<repo-root>/tasks`. Status is the folder; `git mv` is the audit trail; the
//! ticket format is a MiniJinja template you can edit.

mod board;
mod branch;
mod form;
mod git;
mod render;
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

Every task carries labels: a type (feat, fix, docs, … — the conventional
commit types) plus optional epic and story labels. The type decides the
branch, so `amd start` on a feat titled \"Add login\" creates and switches to
feature/add-login.

Environment:
  AMD_DIR       board directory name under the repository root (default: tasks)
  AMD_TYPES     comma-separated type labels (default: conventional commits)
  AMD_YES       set to 1 to create a missing board without prompting
  AMD_NO_INPUT  set to 1 to never prompt (same as --no-input)
  AMD_NO_BRANCH set to 1 to never create branches (same as --no-branch)
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
    /// Plain text output instead of the rich board
    #[arg(long, global = true)]
    plain: bool,
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
    /// Move a task todo -> doing, and switch to its branch
    Start {
        #[arg(value_name = "REF")]
        task: Option<String>,
        /// Branch to create instead of the one in the ticket
        #[arg(short, long, value_name = "NAME")]
        branch: Option<String>,
        /// Move the task without touching branches
        #[arg(long)]
        no_branch: bool,
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
    /// List the epics on the board, or one epic's tasks
    Epics {
        #[arg(value_name = "EPIC")]
        epic: Option<String>,
    },
    /// List the stories on the board, or one story's tasks
    Stories {
        #[arg(value_name = "STORY")]
        story: Option<String>,
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
    /// Type label: a conventional-commit type (feat, fix, docs, chore, …)
    #[arg(long, value_name = "TYPE")]
    r#type: Option<String>,
    /// Epic label this task belongs to
    #[arg(short = 'E', long, value_name = "EPIC")]
    epic: Option<String>,
    /// Story label this task belongs to
    #[arg(short = 'S', long, value_name = "STORY")]
    story: Option<String>,
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
    render::set_plain(cli.plain);
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
        Cmd::Start {
            task,
            branch,
            no_branch,
        } => {
            let task = resolve(&board, task, "start", &[Column::Todo])?;
            cmd_start(&board, &task, branch, no_branch)
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
        Cmd::Epics { epic } => cmd_label(&board, "epic", epic),
        Cmd::Stories { story } => cmd_label(&board, "story", story),
        Cmd::Templates { command } => cmd_templates(&board, command.unwrap_or(TemplateCmd::List)),
    }
}

/// Move a task into doing/ and put the working tree on its branch.
///
/// Order matters: the `git mv` is staged first so it travels with the switch
/// and lands on the task's own branch.
fn cmd_start(
    board: &Board,
    task: &task::Task,
    override_branch: Option<String>,
    no_branch: bool,
) -> Result<()> {
    // Read the branch before the move — afterwards the path is stale.
    let wanted = match override_branch {
        Some(name) => Some(name),
        None => match task.branch() {
            Some(name) => Some(name),
            // Older tasks (and custom templates) may carry only a type label.
            None => match task.kind() {
                Some(kind) => Some(branch::for_title(&kind, &task.title())?),
                None => None,
            },
        },
    };

    board.move_task(task, Column::Doing)?;

    if no_branch || std::env::var("AMD_NO_BRANCH").as_deref() == Ok("1") {
        return Ok(());
    }
    let Some(name) = wanted else {
        eprintln!("amd: no type or branch on this task; left the branch alone");
        return Ok(());
    };
    branch::validate(&name)?;
    if git::current_branch(&board.root).as_deref() == Some(name.as_str()) {
        println!("already on {name}");
        return Ok(());
    }
    let exists = git::branch_exists(&board.root, &name);
    git::switch_branch(&board.root, &name, !exists)?;
    println!("{} {name}", if exists { "switched to" } else { "branch" });
    Ok(())
}

/// Ask for a grouping label, completing against the ones already in use.
fn label(board: &Board, name: &str, given: Option<String>, full_form: bool) -> Result<String> {
    match given {
        Some(value) => Ok(value.trim().to_string()),
        None if full_form => {
            let known = board.label_values(name)?;
            let message = format!("{}:", templates::field_label(name));
            form::label(
                &message,
                "",
                known,
                "optional label grouping tasks; blank for none",
            )
        }
        None => Ok(String::new()),
    }
}

/// `amd epics` / `amd stories`: the index for a grouping label, or one
/// value's tasks. Progress is counted from the columns, so it's always current.
fn cmd_label(board: &Board, label: &str, value: Option<String>) -> Result<()> {
    let tasks = board.tasks()?;
    let of = |task: &task::Task| task.meta(label).filter(|value| !value.is_empty());

    if let Some(value) = value {
        let mut found = false;
        for task in tasks
            .iter()
            .filter(|task| of(task).as_deref() == Some(&value))
        {
            found = true;
            println!(
                "  [{}] {} ({}/)",
                task.id_display(),
                task.title(),
                task.column
            );
        }
        if !found {
            bail!("no tasks with {label} '{value}'");
        }
        return Ok(());
    }

    let values = board.label_values(label)?;
    if values.is_empty() {
        println!("(no {label}s — add one with: amd new \"…\" --{label} <name>)");
        return Ok(());
    }
    let width = values.iter().map(String::len).max().unwrap_or(0);
    for value in values {
        let mine: Vec<&task::Task> = tasks
            .iter()
            .filter(|task| of(task).as_deref() == Some(value.as_str()))
            .collect();
        let done = mine
            .iter()
            .filter(|task| task.column == Column::Done)
            .count();
        println!("{value:width$}  {done}/{} done", mine.len());
    }
    Ok(())
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

    // The type label doubles as the branch prefix, so it's asked first: the
    // title validator needs it to show the branch the title will produce.
    let kind = match args.r#type {
        Some(kind) => {
            branch::validate_type(&kind)?;
            kind
        }
        None if full_form => form::select("Type:", branch::types(), &branch::default_type())?,
        None => branch::default_type(),
    };

    let title = match args.title {
        Some(title) if !args.interactive => title,
        title => form::title(&kind, title.as_deref())?,
    };
    // Non-interactively this is where a title that can't become a branch stops.
    branch::validate_title(&kind, &title)?;
    let branch_name = branch::for_title(&kind, &title)?;

    let epic = label(board, "epic", args.epic, full_form)?;
    let story = label(board, "story", args.story, full_form)?;

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
        kind,
        epic,
        story,
        branch: branch_name.clone(),
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
    println!("created {}/{id}-{slug}.md ({branch_name})", Column::Todo);
    if args.edit {
        open_editor(&path)?;
    }
    Ok(())
}

fn cmd_ls(board: &Board, column: &str) -> Result<()> {
    if column.eq_ignore_ascii_case("all") {
        return render::columns(board, &Column::ALL);
    }
    let column = Column::parse(column)
        .with_context(|| format!("unknown column '{column}' (todo, doing, done or all)"))?;
    render::columns(board, &[column])
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
