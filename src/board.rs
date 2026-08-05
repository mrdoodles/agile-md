//! The board: `<repo-root>/${AMD_DIR:-tasks}` with a directory per column.
//!
//! Status is the folder, so "moving a task" is a rename — `git mv` when the
//! file is tracked, so the history is the audit trail.

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

use crate::git;
use crate::task::Task;
use crate::templates;

/// Default board directory name; override with `AMD_DIR`.
const DEFAULT_DIR: &str = "tasks";

/// Where junked tickets go. Not a column: it's off the board, and its contents
/// are gitignored, so junking a ticket takes it out of the history rather than
/// recording a fourth status.
const JUNK: &str = "junk";

/// Keeps the junk drawer itself in git while ignoring what's in it.
const JUNK_GITIGNORE: &str = "*\n!.gitignore\n";

/// The board's id counter: the number the next ticket will take. One line,
/// tracked with the board, so ids keep climbing even when tickets are deleted
/// outright rather than junked — a scan can only see what's still there.
const COUNTER: &str = ".next-id";

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
    /// A board at an explicit path, for tests and for a caller handed one
    /// rather than discovering it.
    pub fn at(root: PathBuf) -> Board {
        Board { root }
    }

    /// The board inside a repository whose root we already know — a registered
    /// one, rather than the repository we happen to be standing in.
    pub fn in_repo(repo: &std::path::Path) -> Board {
        Board {
            root: repo.join(dir_name()),
        }
    }
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

    /// Open an existing board, or say where it should have been. Creating one
    /// is a decision for the caller — the CLI asks, the GUI offers a button.
    pub fn open() -> Result<Board> {
        let board = Board::locate()?;
        if board.root.is_dir() {
            return Ok(board);
        }
        bail!(
            "no board at {} — run 'amd init' (or set AMD_YES=1)",
            board.root.display()
        )
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

    pub fn junk_dir(&self) -> PathBuf {
        self.root.join(JUNK)
    }

    pub fn counter_path(&self) -> PathBuf {
        self.root.join(COUNTER)
    }

    /// What the counter file says, if it says anything sensible.
    fn stored_next_id(&self) -> Option<u32> {
        fs::read_to_string(self.counter_path())
            .ok()?
            .trim()
            .parse::<u32>()
            .ok()
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
        // The junk drawer is tracked, its contents are not.
        let junk = self.junk_dir();
        fs::create_dir_all(&junk).with_context(|| format!("creating {}", junk.display()))?;
        let ignore = junk.join(".gitignore");
        if !ignore.exists() {
            fs::write(&ignore, JUNK_GITIGNORE)
                .with_context(|| format!("creating {}", ignore.display()))?;
        }

        let counter = self.counter_path();
        if !counter.exists() {
            fs::write(&counter, "1\n")
                .with_context(|| format!("creating {}", counter.display()))?;
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

    /// Every tag already in use on the board, sorted and deduplicated — the
    /// suggestions offered when a task is created interactively.
    pub fn tags(&self) -> Result<Vec<String>> {
        let mut tags: Vec<String> = Vec::new();
        for task in self.tasks()? {
            let Some(raw) = task.meta("tags") else {
                continue;
            };
            for tag in raw.trim_matches(['[', ']']).split(',') {
                let tag = tag.trim().trim_matches(['"', '\'']).to_string();
                if !tag.is_empty() && !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
        }
        tags.sort();
        Ok(tags)
    }

    /// Everyone with a ticket here, sorted — the suggestions when assigning
    /// one.
    pub fn assignees(&self) -> Result<Vec<String>> {
        let mut assignees: Vec<String> = Vec::new();
        for task in self.tasks()? {
            if let Some(who) = task.assignee()
                && !assignees.contains(&who)
            {
                assignees.push(who);
            }
        }
        assignees.sort();
        Ok(assignees)
    }

    /// Every task's `NNN-slug`, the strings `amd new` completes a related ref
    /// against.
    pub fn stems(&self) -> Result<Vec<String>> {
        Ok(self.tasks()?.into_iter().map(|task| task.stem).collect())
    }

    /// Tickets that have been junked. Not part of the board, but they still
    /// hold ids.
    pub fn junked(&self) -> Result<Vec<Task>> {
        let dir = self.junk_dir();
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
            // Junked tickets keep the column they were last in; nothing reads
            // it, but it means a restored file lands somewhere sensible.
            if path.is_file()
                && let Some(task) = Task::from_path(&path, Column::Todo)
            {
                tasks.push(task);
            }
        }
        Ok(tasks)
    }

    /// The id the next ticket will take: whichever is higher of the counter
    /// file and one past the highest id on the board.
    ///
    /// The counter is the authority — it remembers ids whose tickets have gone,
    /// junked or deleted, which no scan can. The scan is the safety net: a
    /// lost, stale or badly merged counter can then never hand out an id that
    /// is already in use, and a ticket added by hand still pushes it along.
    /// Only the columns are scanned; the junk drawer is off the board and the
    /// counter already accounts for it.
    pub fn next_id(&self) -> Result<u32> {
        let highest = self
            .tasks()?
            .iter()
            .filter_map(|task| task.id)
            .max()
            .unwrap_or(0);
        Ok(self.stored_next_id().unwrap_or(0).max(highest + 1))
    }

    /// Record that `used` has been taken, so the next ticket gets the one
    /// after. Called once the ticket is on disk, never when one is merely
    /// drafted — an abandoned draft should leave no gap.
    pub fn record_id(&self, used: u32) -> Result<()> {
        let next = used.saturating_add(1);
        if self.stored_next_id().is_some_and(|stored| stored >= next) {
            return Ok(());
        }
        let path = self.counter_path();
        fs::write(&path, format!("{next}\n")).with_context(|| format!("writing {}", path.display()))
    }

    /// Take a ticket off the board. The junk directory is gitignored, so a
    /// tracked ticket leaves the index (`git rm --cached`) and then moves —
    /// `git mv` would refuse the ignored destination, and forcing it would put
    /// the junk back into the history the .gitignore is there to keep it out of.
    pub fn junk(&self, task: &Task) -> Result<PathBuf> {
        let dir = self.junk_dir();
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let dest = dir.join(task.file_name());
        if dest.exists() {
            bail!("{} already exists", dest.display());
        }
        if git::is_tracked(&self.root, &task.path) {
            git::untrack(&self.root, &task.path)?;
        }
        fs::rename(&task.path, &dest).with_context(|| format!("moving {}", task.path.display()))?;
        Ok(dest)
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

    fn board_at(dir: &std::path::Path) -> Board {
        let board = Board::at(dir.join("tasks"));
        board.create().expect("board");
        board
    }

    fn touch(board: &Board, name: &str) {
        fs::write(board.dir(Column::Todo).join(name), "---\n---\n").unwrap();
    }

    #[test]
    fn a_new_board_starts_at_one() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        assert_eq!(board.next_id().unwrap(), 1);
        assert_eq!(
            fs::read_to_string(board.counter_path()).unwrap().trim(),
            "1"
        );
    }

    #[test]
    fn the_counter_moves_on_as_tickets_are_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        touch(&board, "001-one.md");
        board.record_id(1).unwrap();
        assert_eq!(board.next_id().unwrap(), 2);
        touch(&board, "002-two.md");
        board.record_id(2).unwrap();
        assert_eq!(board.next_id().unwrap(), 3);
    }

    #[test]
    fn a_deleted_ticket_does_not_hand_its_id_back() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        touch(&board, "001-one.md");
        board.record_id(1).unwrap();
        // Deleted outright, not junked: nothing on disk remembers it.
        fs::remove_file(board.dir(Column::Todo).join("001-one.md")).unwrap();
        assert_eq!(board.next_id().unwrap(), 2, "the counter still knows");
    }

    #[test]
    fn a_lost_or_stale_counter_cannot_collide() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        touch(&board, "007-seven.md");
        // Counter behind what's on disk — a bad merge, or a hand-added ticket.
        fs::write(board.counter_path(), "3\n").unwrap();
        assert_eq!(board.next_id().unwrap(), 8);
        // Counter missing entirely.
        fs::remove_file(board.counter_path()).unwrap();
        assert_eq!(board.next_id().unwrap(), 8);
    }

    #[test]
    fn recording_an_older_id_does_not_wind_the_counter_back() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        board.record_id(9).unwrap();
        board.record_id(2).unwrap();
        assert_eq!(board.next_id().unwrap(), 10);
    }

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
