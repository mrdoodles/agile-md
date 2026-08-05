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
use crate::task::Task;
use crate::templates::{self, Templates};
use crate::{branch, render};

use settings::Settings;

const HELP: &str = "j/k move  h/l column  [ ] shift  enter view  e edit  n new  s settings  q quit";
const SETTINGS_BUTTON: &str = " settings ";

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
    New {
        title: String,
        kind: usize,
    },
    Settings {
        selected: usize,
        previous: ThemeName,
    },
}

struct App {
    board: Board,
    templates: Templates,
    settings: Settings,
    /// Flattened columns: the task and how deep it is nested.
    columns: Vec<Vec<(Task, usize)>>,
    /// Selection per column, so switching columns keeps your place.
    selected: Vec<usize>,
    column: usize,
    mode: Mode,
    status: String,
    quit: bool,
    /// Where things were drawn last frame, so a click can find them.
    column_areas: Vec<Rect>,
    settings_area: Rect,
    dialog_area: Rect,
}

impl App {
    fn new(board: Board) -> Result<App> {
        let templates = Templates::load(&board)?;
        let mut app = App {
            board,
            templates,
            settings: Settings::load(),
            columns: Vec::new(),
            selected: vec![0; Column::ALL.len()],
            column: 0,
            mode: Mode::Board,
            status: String::new(),
            quit: false,
            column_areas: vec![Rect::ZERO; Column::ALL.len()],
            settings_area: Rect::ZERO,
            dialog_area: Rect::ZERO,
        };
        app.reload()?;
        Ok(app)
    }

    fn palette(&self) -> ThemePalette {
        self.settings.theme.palette()
    }

    /// Re-read the board. The files are the source of truth and something else
    /// is probably editing them.
    fn reload(&mut self) -> Result<()> {
        self.templates = Templates::load(&self.board)?;
        self.columns = Column::ALL
            .into_iter()
            .map(|column| {
                Ok(render::flatten(&render::column_forest(
                    &self.board,
                    column,
                )?))
            })
            .collect::<Result<Vec<_>>>()?;
        for (index, tasks) in self.columns.iter().enumerate() {
            self.selected[index] = self.selected[index].min(tasks.len().saturating_sub(1));
        }
        Ok(())
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

    fn current(&self) -> Option<&Task> {
        self.columns
            .get(self.column)
            .and_then(|tasks| tasks.get(self.selected[self.column]))
            .map(|(task, _)| task)
    }

    fn key(&mut self, key: KeyEvent, terminal: &mut DefaultTerminal) -> Result<()> {
        match &mut self.mode {
            Mode::New { title, kind } => match key.code {
                KeyCode::Esc => self.mode = Mode::Board,
                KeyCode::Enter => {
                    let (title, kind) = (title.clone(), *kind);
                    self.create(title, kind);
                }
                KeyCode::Tab | KeyCode::Down | KeyCode::Right => {
                    *kind = (*kind + 1) % branch::ticket_types().len().max(1);
                }
                KeyCode::BackTab | KeyCode::Up | KeyCode::Left => {
                    let count = branch::ticket_types().len().max(1);
                    *kind = (*kind + count - 1) % count;
                }
                KeyCode::Backspace => {
                    title.pop();
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    title.push(ch);
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
                _ => self.move_selection(1),
            },
            MouseEventKind::ScrollUp => match &mut self.mode {
                Mode::Detail { scroll, .. } => *scroll = scroll.saturating_sub(1),
                Mode::Settings { selected, .. } => {
                    let count = ThemeName::all().len();
                    *selected = (*selected + count - 1) % count;
                    self.preview();
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
                    && row < ThemeName::all().len()
                {
                    *selected = row;
                    self.preview();
                }
            }
            // Clicking the type line cycles it, the way a dropdown would.
            Mode::New { kind, .. } => {
                if self.dialog_area.contains(at) {
                    *kind = (*kind + 1) % branch::ticket_types().len().max(1);
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
                    if let Some(row) = row_in(area, at)
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
        if let Some(task) = self.current() {
            match std::fs::read_to_string(&task.path) {
                Ok(body) => self.mode = Mode::Detail { body, scroll: 0 },
                Err(err) => self.status = format!("{err}"),
            }
        }
    }

    fn open_new(&mut self) {
        self.mode = Mode::New {
            title: String::new(),
            kind: branch::ticket_types()
                .iter()
                .position(|kind| *kind == branch::default_type())
                .unwrap_or(0),
        };
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
        let Some(task) = self.current().cloned() else {
            return;
        };
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
            app.board.move_task(&task, to)?;
            app.reload()?;
            app.column = Column::ALL.iter().position(|c| *c == to).unwrap_or(0);
            Ok(format!("moved {} to {to}/", task.id_display()))
        });
    }

    /// Hand the ticket to `$EDITOR`, giving the terminal back while it runs.
    fn edit(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(task) = self.current().cloned() else {
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

    fn create(&mut self, title: String, kind: usize) {
        let chosen = branch::ticket_types()
            .get(kind)
            .cloned()
            .unwrap_or_else(branch::default_type);
        self.attempt(move |app| {
            // One answer, two settings: the type picks the template as well.
            let (template, kind) = branch::resolve(&chosen);
            let draft = Draft::prepare(
                &app.board,
                &app.templates,
                NewTicket {
                    title,
                    template,
                    kind,
                    ..Default::default()
                },
            )?;
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
            self.draw_column(frame, *area, index, &palette);
        }
        self.draw_footer(frame, footer, &palette);

        match &self.mode {
            Mode::Detail { body, scroll } => {
                let (body, scroll) = (body.clone(), *scroll);
                self.draw_detail(frame, &body, scroll, &palette);
            }
            Mode::New { title, kind } => {
                let (title, kind) = (title.clone(), *kind);
                self.draw_new(frame, &title, kind, &palette);
            }
            Mode::Settings { selected, .. } => {
                let selected = *selected;
                self.draw_settings(frame, selected, &palette);
            }
            Mode::Board => self.dialog_area = Rect::ZERO,
        }
    }

    fn draw_footer(&mut self, frame: &mut Frame, area: Rect, palette: &ThemePalette) {
        let button = SETTINGS_BUTTON.chars().count() as u16;
        let [text, settings] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(button)]).areas(area);
        self.settings_area = settings;

        let message = if self.status.is_empty() {
            HELP.to_string()
        } else {
            format!("{}   —   {HELP}", self.status)
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

    fn draw_column(&self, frame: &mut Frame, area: Rect, index: usize, palette: &ThemePalette) {
        let column = Column::ALL[index];
        let tasks = &self.columns[index];
        let active = index == self.column && matches!(self.mode, Mode::Board);

        let items: Vec<ListItem> = if tasks.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "(empty)",
                Style::new().fg(palette.muted),
            )))]
        } else {
            tasks
                .iter()
                .map(|(task, depth)| {
                    let labels = labels(task);
                    let mut spans = vec![
                        Span::raw("  ".repeat(*depth)),
                        Span::styled(
                            format!("[{}] ", task.id_display()),
                            Style::new().fg(palette.muted),
                        ),
                        Span::styled(task.title(), Style::new().fg(palette.fg)),
                    ];
                    if !labels.is_empty() {
                        spans.push(Span::styled(
                            format!("  {labels}"),
                            Style::new().fg(palette.secondary),
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

        let mut state = ListState::default();
        if !tasks.is_empty() && active {
            state.select(Some(self.selected[index]));
        }
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn draw_detail(&mut self, frame: &mut Frame, body: &str, scroll: u16, palette: &ThemePalette) {
        let area = centred(frame.area(), 70, 80);
        self.dialog_area = area;
        frame.render_widget(Clear, area);
        let title = self
            .current()
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

    fn draw_new(&mut self, frame: &mut Frame, title: &str, kind: usize, palette: &ThemePalette) {
        let area = centred(frame.area(), 60, 20);
        self.dialog_area = area;
        frame.render_widget(Clear, area);
        let types = branch::ticket_types();
        let chosen = types.get(kind).cloned().unwrap_or_default();
        let text = vec![
            Line::from(vec![
                Span::styled("Title: ", Style::new().fg(palette.muted)),
                Span::styled(title.to_string(), Style::new().fg(palette.fg)),
                Span::styled("▏", Style::new().fg(palette.accent)),
            ]),
            Line::from(vec![
                Span::styled("Type:  ", Style::new().fg(palette.muted)),
                Span::styled(
                    format!(" {chosen} ▾ "),
                    Style::new().fg(palette.bg).bg(palette.accent),
                ),
                Span::styled("  tab or click to change", Style::new().fg(palette.muted)),
            ]),
        ];
        frame.render_widget(
            Paragraph::new(text)
                .style(Style::new().bg(palette.bg))
                .block(dialog(
                    palette,
                    " New ticket ",
                    " enter create  esc cancel ",
                )),
            area,
        );
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
        let mut state = ListState::default();
        state.select(Some(selected));
        frame.render_stateful_widget(list, area, &mut state);
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

/// Which list row a click landed on, allowing for the widget's border.
fn row_in(area: Rect, at: Position) -> Option<usize> {
    if !area.contains(at) {
        return None;
    }
    let first = area.y.saturating_add(1);
    (at.y >= first).then(|| (at.y - first) as usize)
}

/// The ticket's type, as shown beside its title.
fn labels(task: &Task) -> String {
    [
        task.ticket()
            .filter(|ticket| ticket != templates::DEFAULT_TEMPLATE),
        task.kind(),
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
    fn the_type_list_leads_with_admin_and_defaults_to_a_change_type() {
        let types = branch::ticket_types();
        assert_eq!(types.first().map(String::as_str), Some("admin"));
        assert!(types.contains(&branch::default_type()));
    }
}
