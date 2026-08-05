//! The terminal front end: the board as a full-screen TUI, built on `ratatui`.
//!
//! The only front end there is. No window, no GPU context, it works over SSH,
//! and it blocks on `read()` so looking at the board costs nothing.
//!
//! It owns no board logic: [`Draft`] creates, [`Board::move_task`] moves,
//! [`render::column_forest`] nests. Editing a ticket leaves the alternate
//! screen and hands over to `$EDITOR`, rather than reimplementing an editor in
//! a list widget.
//!
//! Colours all come from the theme's palette, so changing theme is one lookup
//! rather than hunting down hard-coded `Color`s.

mod settings;

use std::io;
use std::process::Command;

use anyhow::{Context, Result};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use ratatui_themes::{ThemeName, ThemePalette};

use crate::board::{Board, Column};
use crate::create::{Draft, NewTicket};
use crate::registry::{Entry, Registry};
use crate::task::Task;
use crate::templates::{self, Templates};
use crate::{branch, render};

use settings::Settings;

const HELP: &str = "j/k move  h/l column  [ ] shift  enter view  e edit  n new  p repo  a assignee  s settings  q quit";
const SETTINGS_BUTTON: &str = " settings ";
/// The filter entry standing for "nobody has this one".
const UNASSIGNED: &str = "(unassigned)";
/// The filter entry standing for "all of them".
const EVERYTHING: &str = "(all)";

/// Open the board in the terminal. Returns when the user quits.
pub fn run(board: Board) -> Result<()> {
    let mut app = App::new(board)?;
    let mut terminal = enter()?;
    let outcome = app.run(&mut terminal);
    leave();
    outcome
}

/// Take the terminal, including mouse reporting.
fn enter() -> Result<DefaultTerminal> {
    let terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture).context("enabling mouse capture")?;
    Ok(terminal)
}

/// Give it back. Mouse reporting goes before the alternate screen does, or the
/// shell you return to starts receiving escape sequences.
fn leave() {
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
}

/// What the user is doing.
enum Mode {
    Board,
    Detail {
        body: String,
        scroll: u16,
    },
    New(NewForm),
    Settings {
        selected: usize,
        previous: ThemeName,
    },
    /// Pick which repository's board to look at, or all of them.
    Repos {
        selected: usize,
    },
    /// Pick whose tickets to look at, or everyone's.
    Assignees {
        selected: usize,
    },
}

/// The new-ticket form: every field a ticket has, in one dialog, so what a
/// ticket *is* is visible at the moment you make one.
#[derive(Default)]
struct NewForm {
    title: String,
    /// Index into `branch::choices()`, which starts at "(none)".
    branch_type: usize,
    assignee: String,
    parent: String,
    related: String,
    tags: String,
    /// Which row has the cursor.
    focus: usize,
}

/// The rows, in order. The branch type is a dropdown; the rest are text.
const FIELDS: [&str; 6] = [
    "Title",
    "Branch type",
    "Assignee",
    "Parent",
    "Related",
    "Tags",
];
const BRANCH_TYPE_ROW: usize = 1;

impl NewForm {
    fn field_mut(&mut self, row: usize) -> Option<&mut String> {
        match row {
            0 => Some(&mut self.title),
            2 => Some(&mut self.assignee),
            3 => Some(&mut self.parent),
            4 => Some(&mut self.related),
            5 => Some(&mut self.tags),
            _ => None,
        }
    }

    fn value(&self, row: usize) -> String {
        match row {
            0 => self.title.clone(),
            1 => branch::choices()
                .get(self.branch_type)
                .cloned()
                .unwrap_or_default(),
            2 => self.assignee.clone(),
            3 => self.parent.clone(),
            4 => self.related.clone(),
            5 => self.tags.clone(),
            _ => String::new(),
        }
    }

    fn cycle(&mut self, by: i32) {
        let count = branch::choices().len().max(1) as i32;
        self.branch_type = ((self.branch_type as i32 + by).rem_euclid(count)) as usize;
    }

    /// The branch this ticket would get, shown live so the effect of the
    /// dropdown is visible before anything is created.
    fn branch_preview(&self) -> String {
        let kind = branch::normalise(&self.value(BRANCH_TYPE_ROW));
        match branch::for_title(&kind, &self.title) {
            Ok(name) if !name.is_empty() => name,
            _ => "(no branch)".to_string(),
        }
    }
}

/// A ticket on the board, and which registered repository it belongs to —
/// ids restart at 001 per repository, so a card without its origin is
/// ambiguous the moment two boards are shown together.
struct Card {
    task: Task,
    depth: usize,
    repo: usize,
}

struct App {
    board: Board,
    templates: Templates,
    settings: Settings,
    /// Every repository whose board can be shown, the one you're standing in
    /// first.
    repos: Vec<Entry>,
    /// Which repository is on show, or `None` for all of them.
    repo: Option<usize>,
    /// Whose tickets are on show, or `None` for everyone's.
    assignee_filter: Option<String>,
    /// Everyone with a ticket anywhere in view, for the picker.
    assignees: Vec<String>,
    /// Flattened columns of cards.
    columns: Vec<Vec<Card>>,
    /// Selection per column, so switching columns keeps your place.
    selected: Vec<usize>,
    column: usize,
    mode: Mode,
    status: String,
    quit: bool,
    /// Where things were drawn last frame, so a click can find them.
    column_areas: Vec<Rect>,
    /// One list state per column, kept between frames: ratatui writes the
    /// scroll offset into it, and a click needs that offset to work out which
    /// ticket is under the pointer.
    list_states: Vec<ListState>,
    /// The same, for whichever dialog is open.
    dialog_state: ListState,
    settings_area: Rect,
    dialog_area: Rect,
}

impl App {
    fn new(board: Board) -> Result<App> {
        // The board you're standing in is always in the list, registered or
        // not; the rest come from `amd repos`.
        let repos = Registry::load().boards(Some(&board));
        App::build(board, repos, Settings::load())
    }

    /// The half that takes what it needs, so a test can build one without
    /// reading the user's registry or their theme.
    fn build(board: Board, repos: Vec<Entry>, settings: Settings) -> Result<App> {
        let templates = Templates::load(&board)?;
        let mut app = App {
            board,
            templates,
            settings,
            repos,
            repo: None,
            assignee_filter: None,
            assignees: Vec::new(),
            columns: Vec::new(),
            selected: vec![0; Column::ALL.len()],
            column: 0,
            mode: Mode::Board,
            status: String::new(),
            quit: false,
            column_areas: vec![Rect::ZERO; Column::ALL.len()],
            list_states: vec![ListState::default(); Column::ALL.len()],
            dialog_state: ListState::default(),
            settings_area: Rect::ZERO,
            dialog_area: Rect::ZERO,
        };
        app.reload()?;
        Ok(app)
    }

    fn palette(&self) -> ThemePalette {
        self.settings.theme.palette()
    }

    /// Re-read every repository in view. The files are the source of truth and
    /// something else is probably editing them, so nothing is cached between
    /// refreshes — a few directory listings is all this costs.
    fn reload(&mut self) -> Result<()> {
        self.templates = Templates::load(&self.board)?;
        let showing: Vec<usize> = match self.repo {
            Some(index) => vec![index],
            None => (0..self.repos.len()).collect(),
        };

        let mut assignees: Vec<String> = Vec::new();
        let mut columns: Vec<Vec<Card>> = Vec::new();
        for column in Column::ALL {
            let mut cards = Vec::new();
            for repo in &showing {
                let Some(entry) = self.repos.get(*repo) else {
                    continue;
                };
                let board = entry.board();
                let nodes = render::column_forest(&board, column)?;
                for (task, depth) in render::flatten(&nodes) {
                    if let Some(who) = task.assignee()
                        && !assignees.contains(&who)
                    {
                        assignees.push(who);
                    }
                    if !self.wanted(&task) {
                        continue;
                    }
                    cards.push(Card {
                        task,
                        depth,
                        repo: *repo,
                    });
                }
            }
            columns.push(cards);
        }
        assignees.sort();
        self.assignees = assignees;
        self.columns = columns;
        for (index, cards) in self.columns.iter().enumerate() {
            self.selected[index] = self.selected[index].min(cards.len().saturating_sub(1));
        }
        Ok(())
    }

    /// Does this ticket pass the assignee filter?
    fn wanted(&self, task: &Task) -> bool {
        match &self.assignee_filter {
            None => true,
            Some(who) if who == UNASSIGNED => task.assignee().is_none(),
            Some(who) => task.assignee().as_deref() == Some(who.as_str()),
        }
    }

    /// The board a card belongs to, for moving it.
    fn board_of(&self, card: &Card) -> Board {
        self.repos
            .get(card.repo)
            .map(|entry| entry.board())
            .unwrap_or_else(|| Board::at(self.board.root.clone()))
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|frame| self.draw(frame))?;
            // Blocking read: the TUI costs nothing while it waits.
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => self.key(key, terminal)?,
                Event::Mouse(mouse) => self.mouse(mouse),
                _ => {}
            }
        }
        Ok(())
    }

    fn current(&self) -> Option<&Card> {
        self.columns
            .get(self.column)
            .and_then(|cards| cards.get(self.selected[self.column]))
    }

    fn current_task(&self) -> Option<&Task> {
        self.current().map(|card| &card.task)
    }

    fn key(&mut self, key: KeyEvent, terminal: &mut DefaultTerminal) -> Result<()> {
        match &mut self.mode {
            Mode::New(form) => match key.code {
                KeyCode::Esc => self.mode = Mode::Board,
                KeyCode::Enter => self.create(),
                KeyCode::Tab | KeyCode::Down => {
                    form.focus = (form.focus + 1) % FIELDS.len();
                }
                KeyCode::BackTab | KeyCode::Up => {
                    form.focus = (form.focus + FIELDS.len() - 1) % FIELDS.len();
                }
                KeyCode::Right if form.focus == BRANCH_TYPE_ROW => form.cycle(1),
                KeyCode::Left if form.focus == BRANCH_TYPE_ROW => form.cycle(-1),
                KeyCode::Char(' ') if form.focus == BRANCH_TYPE_ROW => form.cycle(1),
                KeyCode::Backspace => {
                    if let Some(field) = form.field_mut(form.focus) {
                        field.pop();
                    }
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let focus = form.focus;
                    if let Some(field) = form.field_mut(focus) {
                        field.push(ch);
                    }
                }
                _ => {}
            },
            Mode::Settings { selected, previous } => match key.code {
                KeyCode::Esc => {
                    // Leaving without choosing puts the old theme back.
                    self.settings.theme = *previous;
                    self.mode = Mode::Board;
                }
                KeyCode::Enter => {
                    let saved = self.settings.save();
                    self.status = match saved {
                        Ok(()) => format!("theme: {}", self.settings.theme.display_name()),
                        Err(err) => format!("theme set, but not saved: {err:#}"),
                    };
                    self.mode = Mode::Board;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    *selected = (*selected + 1) % ThemeName::all().len();
                    self.preview();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    let count = ThemeName::all().len();
                    *selected = (*selected + count - 1) % count;
                    self.preview();
                }
                _ => {}
            },
            Mode::Repos { selected } => match key.code {
                KeyCode::Esc => self.mode = Mode::Board,
                KeyCode::Enter => {
                    let chosen = *selected;
                    self.choose_repo(chosen);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    *selected = (*selected + 1) % (self.repos.len() + 1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    let count = self.repos.len() + 1;
                    *selected = (*selected + count - 1) % count;
                }
                _ => {}
            },
            Mode::Assignees { selected } => match key.code {
                KeyCode::Esc => self.mode = Mode::Board,
                KeyCode::Enter => {
                    let chosen = *selected;
                    self.choose_assignee(chosen);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    *selected = (*selected + 1) % (self.assignees.len() + 2);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    let count = self.assignees.len() + 2;
                    *selected = (*selected + count - 1) % count;
                }
                _ => {}
            },
            Mode::Detail { scroll, .. } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.mode = Mode::Board,
                KeyCode::Char('j') | KeyCode::Down => *scroll = scroll.saturating_add(1),
                KeyCode::Char('k') | KeyCode::Up => *scroll = scroll.saturating_sub(1),
                KeyCode::Char('e') => self.edit(terminal)?,
                _ => {}
            },
            Mode::Board => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
                KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
                KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
                KeyCode::Char('h') | KeyCode::Left => self.move_column(-1),
                KeyCode::Char('l') | KeyCode::Right => self.move_column(1),
                KeyCode::Char('[') => self.shift(-1),
                KeyCode::Char(']') => self.shift(1),
                KeyCode::Char('r') => self.attempt(|app| {
                    app.reload()?;
                    Ok("reloaded".to_string())
                }),
                KeyCode::Char('n') => self.open_new(),
                KeyCode::Char('p') => self.open_repos(),
                KeyCode::Char('a') => self.open_assignees(),
                KeyCode::Char('s') => self.open_settings(),
                KeyCode::Char('e') => self.edit(terminal)?,
                KeyCode::Enter => self.open_detail(),
                _ => {}
            },
        }
        Ok(())
    }

    /// Clicks and the wheel. The board is a set of lists, so a click is a
    /// column plus a row — which is what the areas from the last frame say.
    fn mouse(&mut self, mouse: MouseEvent) {
        let at = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::ScrollDown => match &mut self.mode {
                Mode::Detail { scroll, .. } => *scroll = scroll.saturating_add(1),
                Mode::Settings { selected, .. } => {
                    *selected = (*selected + 1) % ThemeName::all().len();
                    self.preview();
                }
                Mode::Repos { selected } => *selected = (*selected + 1) % (self.repos.len() + 1),
                Mode::Assignees { selected } => {
                    *selected = (*selected + 1) % (self.assignees.len() + 2);
                }
                _ => self.move_selection(1),
            },
            MouseEventKind::ScrollUp => match &mut self.mode {
                Mode::Detail { scroll, .. } => *scroll = scroll.saturating_sub(1),
                Mode::Settings { selected, .. } => {
                    let count = ThemeName::all().len();
                    *selected = (*selected + count - 1) % count;
                    self.preview();
                }
                Mode::Repos { selected } => {
                    let count = self.repos.len() + 1;
                    *selected = (*selected + count - 1) % count;
                }
                Mode::Assignees { selected } => {
                    let count = self.assignees.len() + 2;
                    *selected = (*selected + count - 1) % count;
                }
                _ => self.move_selection(-1),
            },
            MouseEventKind::Down(MouseButton::Left) => self.click(at),
            _ => {}
        }
    }

    fn click(&mut self, at: Position) {
        if self.settings_area.contains(at) && matches!(self.mode, Mode::Board) {
            self.open_settings();
            return;
        }
        match &mut self.mode {
            Mode::Settings { selected, .. } => {
                if let Some(row) = row_in(self.dialog_area, at)
                    && let Some(row) = self.dialog_state.offset().checked_add(row)
                    && row < ThemeName::all().len()
                {
                    *selected = row;
                    self.preview();
                }
            }
            // Clicking a row focuses it; clicking the dropdown cycles it.
            Mode::New(form) => {
                if let Some(row) = row_in(self.dialog_area, at)
                    && row < FIELDS.len()
                {
                    if form.focus == row && row == BRANCH_TYPE_ROW {
                        form.cycle(1);
                    }
                    form.focus = row;
                }
            }
            Mode::Repos { .. } => {
                if let Some(row) = row_in(self.dialog_area, at)
                    && let Some(row) = self.dialog_state.offset().checked_add(row)
                    && row <= self.repos.len()
                {
                    self.choose_repo(row);
                }
            }
            Mode::Assignees { .. } => {
                if let Some(row) = row_in(self.dialog_area, at)
                    && let Some(row) = self.dialog_state.offset().checked_add(row)
                    && row < self.assignees.len() + 2
                {
                    self.choose_assignee(row);
                }
            }
            Mode::Detail { .. } => {}
            Mode::Board => {
                for index in 0..self.column_areas.len() {
                    let area = self.column_areas[index];
                    if !area.contains(at) {
                        continue;
                    }
                    self.column = index;
                    // The visual row is not the index once a column has
                    // scrolled: the offset ratatui left in the state is.
                    if let Some(row) = row_in(area, at)
                        && let Some(row) = self.list_states[index].offset().checked_add(row)
                        && row < self.columns[index].len()
                    {
                        // Clicking the row that's already selected opens it.
                        if self.selected[index] == row {
                            self.open_detail();
                        } else {
                            self.selected[index] = row;
                        }
                    }
                }
            }
        }
    }

    fn open_detail(&mut self) {
        if let Some(task) = self.current_task() {
            match std::fs::read_to_string(&task.path) {
                Ok(body) => self.mode = Mode::Detail { body, scroll: 0 },
                Err(err) => self.status = format!("{err}"),
            }
        }
    }

    fn open_new(&mut self) {
        // Starts on "(none)": a ticket only gets a branch when you say so.
        self.mode = Mode::New(NewForm::default());
    }

    /// The repository picker: "all" first, then each registered board.
    fn open_repos(&mut self) {
        self.mode = Mode::Repos {
            selected: self.repo.map(|index| index + 1).unwrap_or(0),
        };
    }

    fn choose_repo(&mut self, row: usize) {
        self.repo = (row > 0).then(|| row - 1);
        let label = match self.repo.and_then(|index| self.repos.get(index)) {
            Some(entry) => entry.name.clone(),
            None => "all repositories".to_string(),
        };
        self.mode = Mode::Board;
        self.attempt(move |app| {
            app.reload()?;
            Ok(format!("showing {label}"))
        });
    }

    /// The assignee picker: everyone, then unassigned, then each name in view.
    fn open_assignees(&mut self) {
        let selected = match &self.assignee_filter {
            None => 0,
            Some(who) if who == UNASSIGNED => 1,
            Some(who) => self
                .assignees
                .iter()
                .position(|name| name == who)
                .map(|index| index + 2)
                .unwrap_or(0),
        };
        self.mode = Mode::Assignees { selected };
    }

    fn choose_assignee(&mut self, row: usize) {
        self.assignee_filter = match row {
            0 => None,
            1 => Some(UNASSIGNED.to_string()),
            other => self.assignees.get(other - 2).cloned(),
        };
        let label = self
            .assignee_filter
            .clone()
            .unwrap_or_else(|| "everyone".to_string());
        self.mode = Mode::Board;
        self.attempt(move |app| {
            app.reload()?;
            Ok(format!("showing {label}"))
        });
    }

    fn open_settings(&mut self) {
        self.mode = Mode::Settings {
            selected: ThemeName::all()
                .iter()
                .position(|theme| *theme == self.settings.theme)
                .unwrap_or(0),
            previous: self.settings.theme,
        };
    }

    /// Apply the highlighted theme straight away: choosing colours from a list
    /// of names, blind, is no way to choose colours.
    fn preview(&mut self) {
        if let Mode::Settings { selected, .. } = &self.mode
            && let Some(theme) = ThemeName::all().get(*selected)
        {
            self.settings.theme = *theme;
        }
    }

    fn move_selection(&mut self, by: i32) {
        let len = self.columns[self.column].len();
        if len == 0 {
            return;
        }
        let current = self.selected[self.column] as i32;
        self.selected[self.column] = (current + by).clamp(0, len as i32 - 1) as usize;
    }

    fn move_column(&mut self, by: i32) {
        let count = self.columns.len() as i32;
        self.column = ((self.column as i32 + by).rem_euclid(count)) as usize;
    }

    /// Move the selected ticket a column left or right, following it with the
    /// cursor so the next keypress acts on the same task.
    fn shift(&mut self, by: i32) {
        let Some(card) = self.current() else {
            return;
        };
        let (task, board) = (card.task.clone(), self.board_of(card));
        let to = match (task.column, by) {
            (Column::Todo, 1) => Some(Column::Doing),
            (Column::Doing, 1) => Some(Column::Done),
            (Column::Doing, -1) => Some(Column::Todo),
            (Column::Done, -1) => Some(Column::Doing),
            _ => None,
        };
        let Some(to) = to else {
            self.status = format!("{} is already at the end", task.id_display());
            return;
        };
        self.attempt(move |app| {
            // Moving a card moves it in its own repository, not the one you
            // happen to be standing in.
            board.move_task(&task, to)?;
            app.reload()?;
            app.column = Column::ALL.iter().position(|c| *c == to).unwrap_or(0);
            Ok(format!("moved {} to {to}/", task.id_display()))
        });
    }

    /// Hand the ticket to `$EDITOR`, giving the terminal back while it runs.
    fn edit(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(task) = self.current_task().cloned() else {
            return Ok(());
        };
        let editor = std::env::var("EDITOR")
            .ok()
            .filter(|editor| !editor.is_empty())
            .unwrap_or_else(|| "vi".to_string());
        leave();
        let mut parts = editor.split_whitespace();
        let program = parts.next().unwrap_or("vi");
        let status = Command::new(program)
            .args(parts)
            .arg(&task.path)
            .status()
            .with_context(|| format!("running {editor}"));
        *terminal = enter()?;
        terminal.clear()?;
        match status {
            Ok(status) if status.success() => self.attempt(|app| {
                app.reload()?;
                Ok("saved".to_string())
            }),
            Ok(status) => self.status = format!("{editor} exited with {status}"),
            Err(err) => self.status = format!("{err:#}"),
        }
        self.mode = Mode::Board;
        Ok(())
    }

    fn create(&mut self) {
        let Mode::New(form) = &self.mode else {
            return;
        };
        let ticket = NewTicket {
            title: form.title.trim().to_string(),
            template: templates::DEFAULT_TEMPLATE.to_string(),
            branch_type: branch::normalise(&form.value(BRANCH_TYPE_ROW)),
            assignee: form.assignee.trim().to_string(),
            parent: Some(form.parent.trim().to_string()).filter(|it| !it.is_empty()),
            related: split(&form.related),
            tags: split(&form.tags),
            extra: Default::default(),
        };
        self.attempt(move |app| {
            let draft = Draft::prepare(&app.board, &app.templates, ticket)?;
            let created = draft.write(&app.board, None)?;
            app.reload()?;
            app.mode = Mode::Board;
            app.column = 0;
            Ok(format!("created {}", created.task.file_name()))
        });
    }

    /// Run something fallible, reporting rather than dying: a board viewer that
    /// exits on a bad ticket is no use.
    fn attempt(&mut self, what: impl FnOnce(&mut Self) -> Result<String>) {
        match what(self) {
            Ok(message) => self.status = message,
            Err(err) => self.status = format!("error: {err:#}"),
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let palette = self.palette();
        // Paint the theme's background first; everything else sits on it.
        frame.render_widget(
            Block::default().style(Style::new().bg(palette.bg).fg(palette.fg)),
            frame.area(),
        );

        let [board, footer] =
            Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(frame.area());
        let columns = Layout::horizontal([Constraint::Ratio(1, 3); 3]).split(board);
        for (index, area) in columns.iter().enumerate() {
            self.column_areas[index] = *area;
            // Lifted out and put back so the column and its state can be
            // borrowed at the same time; ratatui updates the offset in place.
            let mut state = std::mem::take(&mut self.list_states[index]);
            self.draw_column(frame, *area, index, &palette, &mut state);
            self.list_states[index] = state;
        }
        self.draw_footer(frame, footer, &palette);

        match &self.mode {
            Mode::Detail { body, scroll } => {
                let (body, scroll) = (body.clone(), *scroll);
                self.draw_detail(frame, &body, scroll, &palette);
            }
            Mode::New(_) => self.draw_new(frame, &palette),
            Mode::Settings { selected, .. } => {
                let selected = *selected;
                self.draw_settings(frame, selected, &palette);
            }
            Mode::Repos { selected } => {
                let selected = *selected;
                self.draw_picker(
                    frame,
                    " Repository ",
                    selected,
                    std::iter::once(EVERYTHING.to_string())
                        .chain(self.repos.iter().map(|entry| entry.name.clone()))
                        .collect(),
                    &palette,
                );
            }
            Mode::Assignees { selected } => {
                let selected = *selected;
                let mut choices = vec![EVERYTHING.to_string(), UNASSIGNED.to_string()];
                choices.extend(self.assignees.clone());
                self.draw_picker(frame, " Assignee ", selected, choices, &palette);
            }
            Mode::Board => self.dialog_area = Rect::ZERO,
        }
    }

    fn draw_footer(&mut self, frame: &mut Frame, area: Rect, palette: &ThemePalette) {
        let button = SETTINGS_BUTTON.chars().count() as u16;
        let [text, settings] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(button)]).areas(area);
        self.settings_area = settings;

        let mut filters = Vec::new();
        if let Some(entry) = self.repo.and_then(|index| self.repos.get(index)) {
            filters.push(entry.name.clone());
        } else if self.repos.len() > 1 {
            filters.push(format!("{} repos", self.repos.len()));
        }
        if let Some(who) = &self.assignee_filter {
            filters.push(format!("@{who}"));
        }
        let filters = match filters.is_empty() {
            true => String::new(),
            false => format!("[{}]  ", filters.join(" ")),
        };
        let message = if self.status.is_empty() {
            format!("{filters}{HELP}")
        } else {
            format!("{filters}{}   —   {HELP}", self.status)
        };
        let style = if self.status.starts_with("error:") {
            Style::new().fg(palette.error).bg(palette.bg)
        } else {
            Style::new().fg(palette.muted).bg(palette.bg)
        };
        frame.render_widget(Paragraph::new(message).style(style), text);
        frame.render_widget(
            Paragraph::new(SETTINGS_BUTTON).style(
                Style::new()
                    .fg(palette.bg)
                    .bg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            settings,
        );
    }

    fn draw_column(
        &self,
        frame: &mut Frame,
        area: Rect,
        index: usize,
        palette: &ThemePalette,
        state: &mut ListState,
    ) {
        let column = Column::ALL[index];
        let tasks = &self.columns[index];
        let active = index == self.column && matches!(self.mode, Mode::Board);

        // With more than one repository in view an id alone is ambiguous, so
        // each card says where it came from.
        let show_repo = self.repo.is_none() && self.repos.len() > 1;
        let items: Vec<ListItem> = if tasks.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "(empty)",
                Style::new().fg(palette.muted),
            )))]
        } else {
            tasks
                .iter()
                .map(|card| {
                    let task = &card.task;
                    let labels = labels(card);
                    let mut spans = vec![Span::raw("  ".repeat(card.depth))];
                    if show_repo && let Some(entry) = self.repos.get(card.repo) {
                        spans.push(Span::styled(
                            format!("{} ", entry.name),
                            Style::new().fg(palette.info),
                        ));
                    }
                    spans.push(Span::styled(
                        format!("[{}] ", task.id_display()),
                        Style::new().fg(palette.muted),
                    ));
                    spans.push(Span::styled(task.title(), Style::new().fg(palette.fg)));
                    if !labels.is_empty() {
                        spans.push(Span::styled(
                            format!("  {labels}"),
                            Style::new().fg(palette.secondary),
                        ));
                    }
                    if let Some(who) = task.assignee() {
                        spans.push(Span::styled(
                            format!("  @{who}"),
                            Style::new().fg(palette.accent),
                        ));
                    }
                    ListItem::new(Line::from(spans))
                })
                .collect()
        };

        let accent = column_colour(column, palette);
        let border = if active {
            Style::new().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(palette.muted)
        };
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border)
                    .style(Style::new().bg(palette.bg))
                    .title(Span::styled(
                        format!(" {}  ({}) ", column.as_str().to_uppercase(), tasks.len()),
                        Style::new().fg(accent).add_modifier(Modifier::BOLD),
                    )),
            )
            .highlight_style(
                Style::new()
                    .bg(palette.selection)
                    .fg(palette.fg)
                    .add_modifier(Modifier::BOLD),
            );

        state.select((!tasks.is_empty() && active).then(|| self.selected[index]));
        frame.render_stateful_widget(list, area, state);
    }

    fn draw_detail(&mut self, frame: &mut Frame, body: &str, scroll: u16, palette: &ThemePalette) {
        let area = centred(frame.area(), 70, 80);
        self.dialog_area = area;
        frame.render_widget(Clear, area);
        let title = self
            .current_task()
            .map(|task| format!(" [{}] {} ", task.id_display(), task.title()))
            .unwrap_or_else(|| " ticket ".to_string());
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0))
                .style(Style::new().bg(palette.bg).fg(palette.fg))
                .block(dialog(palette, &title, " j/k scroll  e edit  esc back ")),
            area,
        );
    }

    /// Every field a ticket has, one row each, with the branch it would get
    /// shown underneath.
    fn draw_new(&mut self, frame: &mut Frame, palette: &ThemePalette) {
        let area = centred(frame.area(), 64, 45);
        self.dialog_area = area;
        frame.render_widget(Clear, area);
        let Mode::New(form) = &self.mode else {
            return;
        };

        let mut lines: Vec<Line> = Vec::new();
        for (row, label) in FIELDS.iter().enumerate() {
            let focused = row == form.focus;
            let value = form.value(row);
            let value_style = if row == BRANCH_TYPE_ROW {
                Style::new().fg(palette.bg).bg(palette.accent)
            } else {
                Style::new().fg(palette.fg)
            };
            let mut spans = vec![Span::styled(
                format!("{label:<12} "),
                Style::new().fg(if focused {
                    palette.accent
                } else {
                    palette.muted
                }),
            )];
            if row == BRANCH_TYPE_ROW {
                spans.push(Span::styled(format!(" {value} ▾ "), value_style));
            } else {
                spans.push(Span::styled(value, value_style));
            }
            if focused {
                spans.push(Span::styled("▏", Style::new().fg(palette.accent)));
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Branch       ", Style::new().fg(palette.muted)),
            Span::styled(form.branch_preview(), Style::new().fg(palette.secondary)),
        ]));

        frame.render_widget(
            Paragraph::new(lines)
                .style(Style::new().bg(palette.bg))
                .block(dialog(
                    palette,
                    " New ticket ",
                    " tab field  ←/→ change  enter create  esc cancel ",
                )),
            area,
        );
    }

    /// One dialog shape for the repository and owner pickers.
    fn draw_picker(
        &mut self,
        frame: &mut Frame,
        title: &str,
        selected: usize,
        choices: Vec<String>,
        palette: &ThemePalette,
    ) {
        let area = centred(frame.area(), 46, 60);
        self.dialog_area = area;
        frame.render_widget(Clear, area);
        let items: Vec<ListItem> = choices
            .iter()
            .map(|choice| {
                ListItem::new(Line::from(Span::styled(
                    choice.clone(),
                    Style::new().fg(palette.fg),
                )))
            })
            .collect();
        let list = List::new(items)
            .block(dialog(palette, title, " enter choose  esc cancel "))
            .style(Style::new().bg(palette.bg))
            .highlight_style(Style::new().bg(palette.selection).fg(palette.fg));
        let mut state = std::mem::take(&mut self.dialog_state);
        state.select(Some(selected.min(choices.len().saturating_sub(1))));
        frame.render_stateful_widget(list, area, &mut state);
        self.dialog_state = state;
    }

    fn draw_settings(&mut self, frame: &mut Frame, selected: usize, palette: &ThemePalette) {
        let area = centred(frame.area(), 46, 70);
        self.dialog_area = area;
        frame.render_widget(Clear, area);
        let current = self.settings.theme;
        let items: Vec<ListItem> = ThemeName::all()
            .iter()
            .map(|theme| {
                let mark = if *theme == current { "● " } else { "  " };
                ListItem::new(Line::from(vec![
                    Span::styled(mark, Style::new().fg(palette.accent)),
                    Span::styled(theme.display_name(), Style::new().fg(palette.fg)),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(dialog(palette, " Theme ", " enter save  esc cancel "))
            .style(Style::new().bg(palette.bg))
            .highlight_style(Style::new().bg(palette.selection).fg(palette.fg));
        let mut state = std::mem::take(&mut self.dialog_state);
        state.select(Some(selected));
        frame.render_stateful_widget(list, area, &mut state);
        self.dialog_state = state;
    }
}

fn dialog<'a>(palette: &ThemePalette, title: &'a str, footer: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(palette.accent))
        .style(Style::new().bg(palette.bg))
        .title(Span::styled(
            title,
            Style::new().fg(palette.accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(footer, Style::new().fg(palette.muted)))
}

/// A comma-separated answer, as the list it stands for.
fn split(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Which list row a click landed on, allowing for the widget's border.
fn row_in(area: Rect, at: Position) -> Option<usize> {
    if !area.contains(at) {
        return None;
    }
    let first = area.y.saturating_add(1);
    (at.y >= first).then(|| (at.y - first) as usize)
}

/// The ticket's branch type, and a marker when its parent is in another column
/// — the same shorthand the printed board uses, so a child doesn't look
/// orphaned just because its parent has moved on.
fn labels(card: &Card) -> String {
    [
        card.task.branch_type(),
        (card.depth == 0)
            .then(|| card.task.parent())
            .flatten()
            .map(|parent| format!("^{parent}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<String>>()
    .join(" ")
}

/// Columns read left-to-right as work progresses; the palette supplies the
/// three colours, so a theme changes them too.
fn column_colour(column: Column, palette: &ThemePalette) -> Color {
    match column {
        Column::Todo => palette.info,
        Column::Doing => palette.warning,
        Column::Done => palette.success,
    }
}

/// A box taking `width`/`height` percent of the screen, centred.
fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let [_, middle, _] = Layout::vertical([
        Constraint::Percentage((100 - height) / 2),
        Constraint::Percentage(height),
        Constraint::Percentage((100 - height) / 2),
    ])
    .areas(area);
    let [_, centre, _] = Layout::horizontal([
        Constraint::Percentage((100 - width) / 2),
        Constraint::Percentage(width),
        Constraint::Percentage((100 - width) / 2),
    ])
    .areas(middle);
    centre
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_maps_to_the_row_under_it() {
        let area = Rect::new(0, 0, 20, 10);
        // The first row sits below the border.
        assert_eq!(row_in(area, Position::new(5, 1)), Some(0));
        assert_eq!(row_in(area, Position::new(5, 4)), Some(3));
        // The border itself, and anything outside, is not a row.
        assert_eq!(row_in(area, Position::new(5, 0)), None);
        assert_eq!(row_in(area, Position::new(50, 5)), None);
    }

    #[test]
    fn every_theme_can_show_a_board() {
        for theme in ThemeName::all() {
            let palette = theme.palette();
            // The board leans on these differing: equal ones would hide the
            // text or the selection entirely.
            assert_ne!(palette.bg, palette.fg, "{}", theme.display_name());
            assert_ne!(palette.bg, palette.selection, "{}", theme.display_name());
        }
    }

    #[test]
    fn the_branch_type_dropdown_starts_at_no_branch() {
        let form = NewForm::default();
        assert_eq!(form.value(BRANCH_TYPE_ROW), branch::NONE);
        assert_eq!(form.branch_preview(), "(no branch)");
    }

    #[test]
    fn the_form_carries_every_ticket_field() {
        assert_eq!(
            FIELDS,
            [
                "Title",
                "Branch type",
                "Assignee",
                "Parent",
                "Related",
                "Tags"
            ]
        );
    }

    #[test]
    fn choosing_a_branch_type_shows_the_branch_it_would_make() {
        let mut form = NewForm {
            title: "Ticket fields".to_string(),
            ..Default::default()
        };
        // (none) -> feature -> bugfix -> hotfix -> release -> chore
        for _ in 0..5 {
            form.cycle(1);
        }
        assert_eq!(form.value(BRANCH_TYPE_ROW), "chore");
        assert_eq!(form.branch_preview(), "chore/ticket-fields");
        // Back round to the start: no branch again.
        form.cycle(1);
        assert_eq!(form.branch_preview(), "(no branch)");
    }

    #[test]
    fn typing_goes_to_the_focused_field() {
        let mut form = NewForm::default();
        form.field_mut(0).unwrap().push_str("Title here");
        form.focus = 2;
        form.field_mut(form.focus).unwrap().push_str("tim");
        assert_eq!(form.title, "Title here");
        assert_eq!(form.assignee, "tim");
        // The dropdown row has no text to type into.
        assert!(form.field_mut(BRANCH_TYPE_ROW).is_none());
    }
}

#[cfg(test)]
mod board_tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::fs;

    /// A board with `count` tickets in todo/, and an App looking at it —
    /// no registry, no saved theme, nothing outside the temp directory.
    fn app(count: usize) -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().expect("temp dir");
        let board = Board::at(dir.path().join("tasks"));
        board.create().expect("board");
        for id in 1..=count {
            let name = format!("{id:03}-ticket-{id}.md");
            let body = format!("---\nid: \"{id:03}\"\ntitle: \"Ticket {id}\"\n---\n");
            fs::write(board.dir(Column::Todo).join(name), body).expect("ticket");
        }
        let entry = Entry::new(dir.path().to_path_buf());
        let app = App::build(board, vec![entry], Settings::default()).expect("app");
        (dir, app)
    }

    /// Draw once into a fixed-size buffer, the way a real terminal would.
    fn draw(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn a_column_scrolls_to_keep_the_selection_in_view() {
        let (_dir, mut app) = app(20);
        // Nine rows of list in a twelve-row terminal, so this is off-screen.
        app.selected[0] = 15;
        let screen = draw(&mut app, 100, 12);
        assert!(
            screen.contains("Ticket 16"),
            "the selection has to be visible"
        );
        assert!(
            app.list_states[0].offset() > 0,
            "the column should have scrolled"
        );
    }

    #[test]
    fn a_click_lands_on_the_ticket_under_the_pointer_not_the_row_number() {
        let (_dir, mut app) = app(20);
        app.selected[0] = 15;
        draw(&mut app, 100, 12);
        let offset = app.list_states[0].offset();
        assert!(offset > 0, "this only means anything once it has scrolled");

        // The top row of the list, just inside the border.
        let area = app.column_areas[0];
        app.click(Position::new(area.x + 2, area.y + 1));
        assert_eq!(
            app.selected[0], offset,
            "clicking the first visible row selects the ticket shown there"
        );
    }

    #[test]
    fn everything_fits_in_a_small_terminal() {
        let (_dir, mut app) = app(20);
        // Nothing here should panic, however little room there is.
        for (width, height) in [(20, 6), (40, 8), (200, 60)] {
            draw(&mut app, width, height);
        }
    }

    #[test]
    fn a_rejected_ticket_keeps_what_you_typed() {
        let (_dir, mut app) = app(3);
        app.mode = Mode::New(NewForm {
            title: "Worth keeping".to_string(),
            parent: "99".to_string(),
            ..Default::default()
        });
        app.create();

        assert!(app.status.contains("no task matches"), "{}", app.status);
        match &app.mode {
            // Still on the form, with the typing intact: an error shouldn't
            // cost you the ticket you were part-way through writing.
            Mode::New(form) => assert_eq!(form.title, "Worth keeping"),
            _ => panic!("the form should still be open"),
        }
        assert_eq!(app.board.tasks().unwrap().len(), 3, "nothing was created");
    }

    #[test]
    fn a_filter_that_hides_everything_leaves_a_usable_board() {
        let (_dir, mut app) = app(5);
        app.selected[0] = 4;
        app.assignee_filter = Some("nobody-has-this-name".to_string());
        app.reload().expect("reload");

        assert!(app.columns[0].is_empty());
        assert!(app.current().is_none(), "nothing to act on, and no panic");
        // The old selection must not survive as an out-of-range index.
        app.move_selection(1);
        app.shift(1);
        let screen = draw(&mut app, 80, 20);
        assert!(screen.contains("(empty)"), "{screen}");
    }

    #[test]
    fn an_empty_board_draws_and_selects_nothing() {
        let (_dir, mut app) = app(0);
        let screen = draw(&mut app, 80, 20);
        assert!(screen.contains("(empty)"), "{screen}");
        assert!(app.current().is_none());
    }
}
