//! The board: `<repo-root>/${AMD_DIR:-tasks}` with a directory per column.
//!
//! Status is the folder, so "moving a task" is a rename — `git mv` when the
//! file is tracked, so the history is the audit trail.

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::git;
use crate::group::{GROUP_FILE, Group};
use crate::task::Task;
use crate::templates;

/// Default board directory name; override with `AMD_DIR`.
const DEFAULT_DIR: &str = "tasks";

/// Where archived tickets go. Not a column: it's off the board, and its
/// contents are gitignored, so archiving takes a ticket out of the history
/// rather than recording another status.
const ARCHIVE: &str = "archive";

/// Keeps the drawer itself in git while ignoring what's in it.
const ARCHIVE_GITIGNORE: &str = "*\n!.gitignore\n";

/// The board's id counter: the number the next ticket will take. One line,
/// tracked with the board, so ids keep climbing even when tickets are deleted
/// outright rather than archived — a scan can only see what's still there.
const COUNTER: &str = ".next-id";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Column {
    Backlog,
    Todo,
    Doing,
    Done,
}

impl Column {
    pub const ALL: [Column; 4] = [Column::Backlog, Column::Todo, Column::Doing, Column::Done];

    pub fn as_str(self) -> &'static str {
        match self {
            Column::Backlog => "backlog",
            Column::Todo => "todo",
            Column::Doing => "doing",
            Column::Done => "done",
        }
    }

    /// The column to the left, if any — `amd back`.
    pub fn left(self) -> Option<Column> {
        match self {
            Column::Backlog => None,
            Column::Todo => Some(Column::Backlog),
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

    pub fn archive_dir(&self) -> PathBuf {
        self.root.join(ARCHIVE)
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
        // The drawer is tracked; what you put in it is not.
        let archive = self.archive_dir();
        fs::create_dir_all(&archive).with_context(|| format!("creating {}", archive.display()))?;
        let ignore = archive.join(".gitignore");
        if !ignore.exists() {
            fs::write(&ignore, ARCHIVE_GITIGNORE)
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
    /// Every ticket in a column, epic folders included.
    ///
    /// The backlog may hold subdirectories — one per epic or sprint — and their
    /// tickets are as real as the loose ones. Anything that counts ids must see
    /// them: a ticket invisible to the scan is an id waiting to be handed out
    /// twice, which silently repoints every reference to it.
    pub fn tasks_in(&self, column: Column) -> Result<Vec<Task>> {
        let dir = self.dir(column);
        let mut tasks = self.tasks_directly_in(&dir, column, None)?;
        for epic in self.epics_in(column)? {
            tasks.extend(self.tasks_directly_in(&dir.join(&epic), column, Some(epic))?);
        }
        tasks.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.stem.cmp(&b.stem)));
        Ok(tasks)
    }

    /// The epic folders of a column, named by directory. Only the backlog has
    /// them: an epic is a way to group work you have not committed to yet, and
    /// letting them appear in every column would make "status is the folder"
    /// ambiguous.
    pub fn epics_in(&self, column: Column) -> Result<Vec<String>> {
        if column != Column::Backlog {
            return Ok(Vec::new());
        }
        let dir = self.dir(column);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err).with_context(|| format!("reading {}", dir.display())),
        };
        let mut epics = Vec::new();
        for entry in entries {
            let path = entry
                .with_context(|| format!("reading {}", dir.display()))?
                .path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|name| name.to_str())
                && !name.starts_with('.')
            {
                epics.push(name.to_string());
            }
        }
        epics.sort();
        Ok(epics)
    }

    fn tasks_directly_in(
        &self,
        dir: &Path,
        column: Column,
        epic: Option<String>,
    ) -> Result<Vec<Task>> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err).with_context(|| format!("reading {}", dir.display())),
        };
        let mut tasks = Vec::new();
        for entry in entries {
            let path = entry
                .with_context(|| format!("reading {}", dir.display()))?
                .path();
            if path.file_name().and_then(|n| n.to_str()) == Some(GROUP_FILE) {
                continue;
            }
            if path.is_file()
                && let Some(mut task) = Task::from_path(&path, column)
            {
                task.epic = epic.clone();
                tasks.push(task);
            }
        }
        Ok(tasks)
    }

    /// Every group in the backlog — epics and sprints alike — read from their
    /// own directories.
    pub fn groups(&self) -> Result<Vec<Group>> {
        let mut groups = Vec::new();
        for name in self.epics_in(Column::Backlog)? {
            groups.push(Group::read(&self.dir(Column::Backlog).join(name))?);
        }
        Ok(groups)
    }

    /// Look one up by directory name.
    pub fn group(&self, name: &str) -> Result<Group> {
        let dir = self.dir(Column::Backlog).join(name);
        if !dir.is_dir() {
            bail!("no group called {name}");
        }
        Group::read(&dir)
    }

    /// Create an epic or a sprint. The directory name is the identity, so a
    /// second group of the same name is a mistake rather than a merge.
    pub fn create_group(&self, group: &Group) -> Result<()> {
        if group.name.trim().is_empty() {
            bail!("a group needs a name");
        }
        if group.dir.exists() {
            bail!("{} already exists", group.name);
        }
        group.save()
    }

    /// File a ticket under an epic, or move it back to the loose backlog with
    /// `None`. The folder is the move; the frontmatter is updated to match, so
    /// a ticket read on its own still says which epic it belongs to.
    pub fn set_epic(&self, task: &Task, epic: Option<&str>) -> Result<PathBuf> {
        // A started sprint takes tickets in and lets them out. Teams do this —
        // more often when they are new — and it is poor practice that skews the
        // charts, but a tool that refuses leaves people editing frontmatter by
        // hand, which skews the charts *and* loses the record. Every move is a
        // `git mv`, so the scope change is dated in the history and a burnup
        // can show it (docs/adr/0009-sprint-scope-and-archiving.md).
        if let Some(name) = epic {
            let group = self.group(name)?;
            // An unsized ticket in a sprint makes the sprint's total a lie,
            // which is the one number a sprint is for. This rule stays.
            if group.is_sprint() && task.points().is_none() {
                bail!("{} needs points before it can go in {name}", task.stem);
            }
        }
        let dir = match epic {
            Some(name) => self.dir(Column::Backlog).join(name),
            None => self.dir(Column::Backlog),
        };
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let dest = dir.join(task.file_name());
        if dest != task.path {
            if dest.exists() {
                bail!("{} already exists", dest.display());
            }
            // Same rule as move_task: `git mv` so history follows the rename
            // when the ticket is tracked, a plain rename when it is not.
            if git::is_tracked(&self.root, &task.path) {
                git::mv(&self.root, &task.path, &dest)?;
            } else {
                fs::rename(&task.path, &dest)
                    .with_context(|| format!("moving {}", task.path.display()))?;
            }
        }
        let moved = Task::from_path(&dest, Column::Backlog)
            .ok_or_else(|| anyhow::anyhow!("{} is not a task", dest.display()))?;
        moved.set_epic_meta(epic.unwrap_or_default())?;
        Ok(dest)
    }

    /// Every task on the board, column order first.
    /// Every ticket belonging to a group, in whatever column it has reached.
    ///
    /// A sprint's scope does not shrink because somebody started work. Counting
    /// only `backlog/` made a sprint's points fall as tickets moved to
    /// `doing/`, which is the opposite of what that number is for.
    ///
    /// Which field answers "what group is this in" depends on where the ticket
    /// is, and the two are not interchangeable. Inside `backlog/` the **folder**
    /// decides — a ticket dragged into an epic directory by hand is in that
    /// epic, frontmatter or no. Outside it there are no group directories, so
    /// the `epic` key written by `set_epic` is the only carrier.
    ///
    /// The archive is deliberately not searched: an archived ticket has left
    /// the board.
    pub fn tasks_in_group(&self, name: &str) -> Result<Vec<Task>> {
        Ok(self
            .tasks()?
            .into_iter()
            .filter(|task| match task.column {
                Column::Backlog => task.epic.as_deref() == Some(name),
                _ => task.epic_meta().as_deref() == Some(name),
            })
            .collect())
    }

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

    /// Tickets that have been archived. Not part of the board, but they still
    /// hold ids.
    pub fn archived(&self) -> Result<Vec<Task>> {
        let dir = self.archive_dir();
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
            // Archived tickets keep the column they were last in; nothing reads
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
    /// archived or deleted, which no scan can. The scan is the safety net: a
    /// lost, stale or badly merged counter can then never hand out an id that
    /// is already in use, and a ticket added by hand still pushes it along.
    /// Only the columns are scanned; the archive is off the board and the
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

    /// Take a ticket off the board. The archive is gitignored, so a
    /// tracked ticket leaves the index (`git rm --cached`) and then moves —
    /// `git mv` would refuse the ignored destination, and forcing it would put
    /// the ticket back into the history the .gitignore is there to keep it out of.
    pub fn archive(&self, task: &Task) -> Result<PathBuf> {
        // Moving a ticket out of a started sprint leaves a `git mv` in the
        // history: the scope change is dated and the points are still readable
        // on disk. Archiving from inside one leaves neither — the file is
        // gitignored and untracked, so the board no longer shows that those
        // points were ever committed to. Take it out of the sprint first; then
        // archive it. Two steps, and the record survives the first.
        if let Some(name) = task.epic.as_deref()
            && let Ok(group) = self.group(name)
            && group.is_sprint()
            && !group.accepts_changes()
        {
            bail!(
                "{} is in {name}, which has started — move it out of the sprint before archiving it",
                task.stem
            );
        }
        let dir = self.archive_dir();
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
    ///
    /// Returns where the ticket landed. It deliberately says nothing: a window
    /// has nowhere to put a line of text, and a library that prints is a
    /// library that has decided what its caller's output looks like
    /// (docs/adr/0004-one-library-two-interfaces.md).
    pub fn move_task(&self, task: &Task, to: Column) -> Result<PathBuf> {
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
        Ok(dest)
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

    fn backlog_ticket(board: &Board, name: &str) {
        let body = format!("---\nid: \"{}\"\ntitle: \"T\"\n---\n\nbody\n", &name[..3]);
        fs::write(board.dir(Column::Backlog).join(name), body).unwrap();
    }

    #[test]
    fn tickets_inside_an_epic_folder_are_still_on_the_board() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        backlog_ticket(&board, "001-loose.md");
        let epic = board.dir(Column::Backlog).join("checkout");
        fs::create_dir_all(&epic).unwrap();
        fs::write(epic.join("002-filed.md"), "---\nid: \"002\"\n---\n").unwrap();

        let tasks = board.tasks_in(Column::Backlog).unwrap();
        assert_eq!(tasks.len(), 2, "the filed ticket must be seen");
        let filed = tasks.iter().find(|t| t.stem == "002-filed").unwrap();
        assert_eq!(filed.epic.as_deref(), Some("checkout"));

        // The reason this matters: an unseen ticket is an id handed out twice.
        assert_eq!(board.next_id().unwrap(), 3);
        assert_eq!(board.epics_in(Column::Backlog).unwrap(), ["checkout"]);
        // Epics are a backlog idea only.
        assert!(board.epics_in(Column::Todo).unwrap().is_empty());
    }

    fn sprint(board: &Board, name: &str) -> crate::group::Group {
        let group = crate::group::Group {
            dir: board.dir(Column::Backlog).join(name),
            name: name.to_string(),
            kind: crate::group::Kind::Sprint,
            description: String::new(),
            days: crate::group::DEFAULT_DAYS,
            state: crate::group::State::Pending,
        };
        board.create_group(&group).unwrap();
        group
    }

    #[test]
    fn a_sprint_refuses_an_unsized_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        sprint(&board, "sprint-1");
        backlog_ticket(&board, "001-unsized.md");

        let task = board.find("001").unwrap();
        let err = board.set_epic(&task, Some("sprint-1")).unwrap_err();
        assert!(
            err.to_string().contains("needs points"),
            "unexpected: {err}"
        );
        assert!(task.path.exists(), "the ticket must not have moved");

        // Sized, it goes in.
        task.set_points("3").unwrap();
        board.set_epic(&task, Some("sprint-1")).unwrap();
        assert_eq!(board.find("001").unwrap().epic.as_deref(), Some("sprint-1"));
    }

    #[test]
    fn an_epic_takes_unsized_tickets() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        fs::create_dir_all(board.dir(Column::Backlog).join("checkout")).unwrap();
        backlog_ticket(&board, "001-unsized.md");
        let task = board.find("001").unwrap();
        // Sizing is a sprint's requirement, not an epic's: an epic is where
        // work goes before anyone has estimated it.
        board.set_epic(&task, Some("checkout")).unwrap();
        assert_eq!(board.find("001").unwrap().epic.as_deref(), Some("checkout"));
    }

    #[test]
    fn a_started_sprint_takes_tickets_in_and_out_but_refuses_an_archive() {
        // ADR-0009. The old rule — a started sprint is immutable — was one
        // `amd rm` wide, and refusing only pushed the change out of the tool's
        // sight. What it defends now is the *record*: a move out is a git mv,
        // an archive is gitignored and is not.
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        let mut group = sprint(&board, "sprint-1");
        backlog_ticket(&board, "001-in.md");
        backlog_ticket(&board, "002-out.md");

        let inside = board.find("001").unwrap();
        inside.set_points("5").unwrap();
        board.set_epic(&inside, Some("sprint-1")).unwrap();
        group.start().unwrap();

        // Tickets go in after the start...
        let outside = board.find("002").unwrap();
        outside.set_points("2").unwrap();
        board
            .set_epic(&outside, Some("sprint-1"))
            .expect("a started sprint takes a sized ticket");

        // ...and come back out again.
        let inside = board.find("001").unwrap();
        board
            .set_epic(&inside, None)
            .expect("a started sprint lets a ticket out");

        // But not straight into the archive.
        let still_in = board.find("002").unwrap();
        let err = board.archive(&still_in).unwrap_err();
        assert!(
            err.to_string().contains("before archiving"),
            "unexpected: {err}"
        );

        // Out of the sprint first, and then it may go.
        let out = board.find("002").unwrap();
        board.set_epic(&out, None).unwrap();
        let out = board.find("002").unwrap();
        board
            .archive(&out)
            .expect("archivable once out of the sprint");
    }

    #[test]
    fn a_sprint_keeps_its_tickets_when_they_leave_the_backlog() {
        // The bug: scope was counted from backlog/ alone, so a sprint's points
        // fell as work started on them.
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        sprint(&board, "sprint-1");
        backlog_ticket(&board, "001-in.md");

        let task = board.find("001").unwrap();
        task.set_points("5").unwrap();
        board.set_epic(&task, Some("sprint-1")).unwrap();
        assert_eq!(board.tasks_in_group("sprint-1").unwrap().len(), 1);

        let task = board.find("001").unwrap();
        board.move_task(&task, Column::Doing).unwrap();
        assert_eq!(
            board.tasks_in_group("sprint-1").unwrap().len(),
            1,
            "a ticket in doing/ still belongs to its sprint"
        );
    }

    #[test]
    fn the_group_file_is_not_a_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        sprint(&board, "sprint-1");
        let tasks = board.tasks_in(Column::Backlog).unwrap();
        assert!(tasks.is_empty(), "_group.md must not count as a ticket");
        assert_eq!(board.next_id().unwrap(), 1, "nor consume an id");
    }

    #[test]
    fn filing_under_an_epic_moves_the_file_and_updates_the_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let board = board_at(dir.path());
        backlog_ticket(&board, "001-loose.md");
        // The group has to exist first: filing onto a name that isn't there
        // would turn a typo into a new epic.
        for name in ["checkout", "payments"] {
            fs::create_dir_all(board.dir(Column::Backlog).join(name)).unwrap();
        }
        let task = board.find("001").unwrap();

        let dest = board.set_epic(&task, Some("checkout")).unwrap();
        assert!(dest.ends_with("backlog/checkout/001-loose.md"), "{dest:?}");
        assert!(!task.path.exists(), "the original must have moved");
        let filed = board.find("001").unwrap();
        assert_eq!(filed.epic_meta().as_deref(), Some("checkout"));
        assert!(
            fs::read_to_string(&filed.path).unwrap().ends_with("body\n"),
            "the body must survive the move"
        );

        // Moving to another epic rewrites the key rather than accumulating.
        board.set_epic(&filed, Some("payments")).unwrap();
        let moved = board.find("001").unwrap();
        assert_eq!(moved.epic_meta().as_deref(), Some("payments"));
        assert_eq!(moved.epic.as_deref(), Some("payments"));

        // And back out to the loose backlog.
        board.set_epic(&moved, None).unwrap();
        let loose = board.find("001").unwrap();
        assert_eq!(loose.epic, None);
        assert_eq!(loose.epic_meta(), None);
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
        // Deleted outright, not archived: nothing on disk remembers it.
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
    fn columns_move_left_until_the_backlog() {
        assert_eq!(Column::Done.left(), Some(Column::Doing));
        assert_eq!(Column::Doing.left(), Some(Column::Todo));
        assert_eq!(Column::Todo.left(), Some(Column::Backlog));
        assert_eq!(Column::Backlog.left(), None);
    }

    #[test]
    fn columns_parse_case_insensitively() {
        assert_eq!(Column::parse("DONE"), Some(Column::Done));
        assert_eq!(Column::parse("todo"), Some(Column::Todo));
        assert_eq!(Column::parse("Backlog"), Some(Column::Backlog));
        assert_eq!(Column::parse("archive"), None);
    }
}
