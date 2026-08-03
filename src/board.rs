//! The board: `<repo-root>/${AMD_DIR:-tasks}` with a directory per column.
//!
//! Status is the folder, so "moving a task" is a rename — `git mv` when the
//! file is tracked, so the history is the audit trail.

use std::env;
use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

use crate::git;
use crate::task::Task;
use crate::templates;

/// Default board directory name; override with `AMD_DIR`.
const DEFAULT_DIR: &str = "tasks";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Column {
    Todo,
    Doing,
    Done,
}

impl Column {
    pub const ALL: [Column; 3] = [Column::Todo, Column::Doing, Column::Done];

    pub fn as_str(self) -> &'static str {
        match self {
            Column::Todo => "todo",
            Column::Doing => "doing",
            Column::Done => "done",
        }
    }

    /// The column to the left, if any — `amd back`.
    pub fn left(self) -> Option<Column> {
        match self {
            Column::Todo => None,
            Column::Doing => Some(Column::Todo),
            Column::Done => Some(Column::Doing),
        }
    }

    pub fn parse(name: &str) -> Option<Column> {
        Column::ALL
            .into_iter()
            .find(|column| column.as_str() == name.to_ascii_lowercase())
    }
}

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct Board {
    /// Absolute path to the board directory itself (`…/tasks`).
    pub root: PathBuf,
}

impl Board {
    /// Where the board *would* live for the current directory. Does not
    /// require it to exist — `amd init` needs the path before the directory.
    pub fn locate() -> Result<Board> {
        let dir = dir_name();
        let repo = git::repo_root().ok_or_else(|| {
            anyhow!("not inside a git repository (agile-md boards live at <repo>/{dir})")
        })?;
        Ok(Board {
            root: repo.join(dir),
        })
    }

    /// Resolve the board, offering to create it when it doesn't exist: prompt
    /// when interactive, create outright with `AMD_YES=1`, and otherwise fail
    /// with a clear message — never block waiting on a pipe.
    pub fn ensure() -> Result<Board> {
        let board = Board::locate()?;
        if board.root.is_dir() {
            return Ok(board);
        }
        let create = if env::var("AMD_YES").as_deref() == Ok("1") {
            true
        } else if io::stdin().is_terminal() {
            let mut err = io::stderr();
            write!(
                err,
                "No task board found at {}\nCreate an empty board here? [y/N] ",
                board.root.display()
            )?;
            err.flush()?;
            let mut reply = String::new();
            io::stdin().read_line(&mut reply)?;
            matches!(reply.trim().to_ascii_lowercase().as_str(), "y" | "yes")
        } else {
            bail!(
                "no board at {} — run 'amd init' (or set AMD_YES=1)",
                board.root.display()
            );
        };
        if !create {
            bail!("no board created");
        }
        board.create()?;
        eprintln!("Created board at {}", board.root.display());
        Ok(board)
    }

    /// The board directory's name (`tasks`), for display and templates.
    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(dir_name)
    }

    pub fn dir(&self, column: Column) -> PathBuf {
        self.root.join(column.as_str())
    }

    pub fn templates_dir(&self) -> PathBuf {
        self.root.join("templates")
    }

    /// Scaffold the columns and the board README. Idempotent.
    pub fn create(&self) -> Result<()> {
        for column in Column::ALL {
            let dir = self.dir(column);
            fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            let keep = dir.join(".gitkeep");
            if !keep.exists() {
                fs::write(&keep, "").with_context(|| format!("creating {}", keep.display()))?;
            }
        }
        let readme = self.root.join("README.md");
        if !readme.exists() {
            let rendered = templates::render_board_readme(self)?;
            fs::write(&readme, rendered)
                .with_context(|| format!("writing {}", readme.display()))?;
        }
        Ok(())
    }

    /// Tasks in one column, ordered by id then filename.
    pub fn tasks_in(&self, column: Column) -> Result<Vec<Task>> {
        let dir = self.dir(column);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err).with_context(|| format!("reading {}", dir.display())),
        };
        let mut tasks = Vec::new();
        for entry in entries {
            let path = entry
                .with_context(|| format!("reading {}", dir.display()))?
                .path();
            if path.is_file()
                && let Some(task) = Task::from_path(&path, column)
            {
                tasks.push(task);
            }
        }
        tasks.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.stem.cmp(&b.stem)));
        Ok(tasks)
    }

    /// Every task on the board, column order first.
    pub fn tasks(&self) -> Result<Vec<Task>> {
        let mut all = Vec::new();
        for column in Column::ALL {
            all.extend(self.tasks_in(column)?);
        }
        Ok(all)
    }

    /// Next id: one past the highest on the board, so ids are stable and never
    /// reused as tasks move between columns.
    pub fn next_id(&self) -> Result<u32> {
        let max = self
            .tasks()?
            .iter()
            .filter_map(|task| task.id)
            .max()
            .unwrap_or(0);
        Ok(max + 1)
    }

    /// Resolve a task reference: a numeric id (`7`, `007`) or a unique slug
    /// substring. Ambiguity is an error, never a guess.
    pub fn find(&self, reference: &str) -> Result<Task> {
        let tasks = self.tasks()?;
        let mut hits: Vec<Task> = if let Ok(id) = reference.parse::<u32>() {
            tasks.into_iter().filter(|t| t.id == Some(id)).collect()
        } else {
            tasks
                .into_iter()
                .filter(|t| t.stem.contains(reference))
                .collect()
        };
        match hits.len() {
            0 => bail!("no task matches '{reference}'"),
            1 => Ok(hits.remove(0)),
            n => {
                let names: Vec<&str> = hits.iter().map(|t| t.stem.as_str()).collect();
                bail!(
                    "'{reference}' matches {n} tasks; be more specific ({})",
                    names.join(", ")
                )
            }
        }
    }

    /// Move a task to another column, preferring `git mv` so the rename is
    /// tracked. Deliberately does not commit — that stays the user's call.
    pub fn move_task(&self, task: &Task, to: Column) -> Result<()> {
        if task.column == to {
            bail!("already in {to}/");
        }
        let dir = self.dir(to);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let dest = dir.join(task.file_name());
        if dest.exists() {
            bail!("{} already exists", dest.display());
        }
        if git::is_tracked(&self.root, &task.path) {
            git::mv(&self.root, &task.path, &dest)?;
        } else {
            fs::rename(&task.path, &dest)
                .with_context(|| format!("moving {}", task.path.display()))?;
        }
        println!("moved {} -> {to}/", task.file_name());
        Ok(())
    }
}

fn dir_name() -> String {
    env::var("AMD_DIR")
        .ok()
        .filter(|dir| !dir.is_empty())
        .unwrap_or_else(|| DEFAULT_DIR.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_move_left_until_todo() {
        assert_eq!(Column::Done.left(), Some(Column::Doing));
        assert_eq!(Column::Doing.left(), Some(Column::Todo));
        assert_eq!(Column::Todo.left(), None);
    }

    #[test]
    fn columns_parse_case_insensitively() {
        assert_eq!(Column::parse("DONE"), Some(Column::Done));
        assert_eq!(Column::parse("todo"), Some(Column::Todo));
        assert_eq!(Column::parse("backlog"), None);
    }
}
