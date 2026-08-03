//! Board rendering.
//!
//! Two renderers, chosen by where the output is going:
//!
//! - **rich** (`richrs`) when stdout is a terminal — bordered tables, coloured
//!   labels, per-column counts.
//! - **plain** when it isn't (a pipe, a file, CI) or with `--plain`/`NO_COLOR`.
//!   That output is the stable, greppable one the test suite asserts on, and
//!   it's what the bash version printed.
//!
//! Anything that pipes `amd board` into `grep` therefore keeps working, and
//! nobody gets box-drawing characters in a log file.

use std::io::{self, IsTerminal};
use std::sync::OnceLock;

use anyhow::Result;
use richrs::prelude::{Column as RichColumn, Console, Style, Table};

use crate::board::{Board, Column};
use crate::task::Task;

static PLAIN: OnceLock<bool> = OnceLock::new();

/// Record the `--plain` flag. Called once, from `main`.
pub fn set_plain(plain: bool) {
    let _ = PLAIN.set(plain);
}

/// Should we draw the rich board?
fn rich() -> bool {
    if *PLAIN.get().unwrap_or(&false) {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    io::stdout().is_terminal()
}

/// Print one or more columns of the board.
pub fn columns(board: &Board, columns: &[Column]) -> Result<()> {
    let mut loaded = Vec::new();
    for column in columns {
        loaded.push((*column, board.tasks_in(*column)?));
    }
    if rich() {
        // Falling back rather than failing: a rendering problem should never
        // stop you seeing the board.
        if rich_columns(&loaded).is_ok() {
            return Ok(());
        }
    }
    plain_columns(&loaded);
    Ok(())
}

/// The stable text form: a heading per column, `  [id] title  (labels)` rows.
fn plain_columns(columns: &[(Column, Vec<Task>)]) {
    for (column, tasks) in columns {
        println!();
        println!("{}", column.as_str().to_uppercase());
        if tasks.is_empty() {
            println!("  (empty)");
            continue;
        }
        for task in tasks {
            let labels = labels(task);
            let suffix = if labels.is_empty() {
                String::new()
            } else {
                format!("  ({labels})")
            };
            println!("  [{}] {}{suffix}", task.id_display(), task.title());
        }
    }
}

fn rich_columns(columns: &[(Column, Vec<Task>)]) -> Result<()> {
    let mut console = Console::new();
    let width = console.width().clamp(40, 120);
    // Size the cells here rather than with the table's own min_width, which
    // draws borders that don't match the content. Padding every cell to the
    // same width also lines the three tables up under each other.
    let available = width.saturating_sub(14); // borders, padding, the id column
    let longest = |measure: fn(&Task) -> usize, floor: usize| {
        columns
            .iter()
            .flat_map(|(_, tasks)| tasks.iter().map(measure))
            .chain([floor])
            .max()
            .unwrap_or(floor)
    };
    let title_width = longest(|task| task.title().chars().count(), 7) // "(empty)"
        .min(available * 3 / 5)
        .max(4);
    let label_width = longest(|task| labels(task).chars().count(), 6) // "labels"
        .min(available.saturating_sub(title_width))
        .max(6);

    console.print("")?;
    for (column, tasks) in columns {
        let mut table = Table::new()
            .title(format!(
                "{}  ({})",
                column.as_str().to_uppercase(),
                tasks.len()
            ))
            .border_style(border_style(*column))
            .header_style(Style::parse("bold").unwrap_or_default());
        table.add_column(RichColumn::new("id "));
        table.add_column(RichColumn::new(fit("task", title_width)));
        table.add_column(RichColumn::new(fit("labels", label_width)));
        if tasks.is_empty() {
            table.add_row_cells(["   ", &fit("(empty)", title_width), &fit("", label_width)]);
        } else {
            for task in tasks {
                table.add_row_cells([
                    fit(&task.id_display(), 3),
                    fit(&task.title(), title_width),
                    fit(&labels(task), label_width),
                ]);
            }
        }
        console.write_segments(&table.render(width))?;
    }
    console.print("")?;
    console.flush()?;
    Ok(())
}

/// Pad or ellipsise a cell to exactly `width` characters.
fn fit(text: &str, width: usize) -> String {
    let length = text.chars().count();
    if length <= width {
        return format!("{text:<width$}");
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// Columns read left-to-right as work progresses, so they're coloured that way.
fn border_style(column: Column) -> Style {
    let spec = match column {
        Column::Todo => "blue",
        Column::Doing => "yellow",
        Column::Done => "green",
    };
    Style::parse(spec).unwrap_or_default()
}

/// `feat epic:checkout story:guest` — the labels, in a fixed order.
fn labels(task: &Task) -> String {
    [
        task.kind(),
        task.scope()
            .filter(|scope| scope != crate::branch::DEFAULT_SCOPE),
        task.epic().map(|epic| format!("epic:{epic}")),
        task.story().map(|story| format!("story:{story}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<String>>()
    .join(" ")
}
