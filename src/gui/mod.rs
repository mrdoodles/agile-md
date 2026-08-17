//! The desktop board.
//!
//! Laid out the opposite way round to a conventional Kanban: each status is a
//! horizontal lane, and the tickets inside it run left to right. So dragging a
//! card **sideways reorders** it within its status, and dragging it **up or
//! down changes** its status. One window shows every registered repository, so
//! work across boards is visible together without a web front end.

use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::board::{Board, Column};
use crate::registry::Registry;
use crate::task::Task;

/// A card being dragged. Carries where it came from, so the drop can tell a
/// reorder from a status change — and refuse a move between repositories,
/// which would need a new id rather than a rename.
#[derive(Clone)]
struct Dragged {
    path: PathBuf,
    repo: usize,
}

/// What a completed drag asks the board to do, applied after the frame so the
/// UI is never mutating files mid-draw.
enum Drop {
    /// Land `path` in `column` of `repo`, at position `index` among the cards
    /// already there.
    Land {
        path: PathBuf,
        repo: usize,
        column: Column,
        index: usize,
    },
}

struct Card {
    task: Task,
    title: String,
    id: String,
    assignee: Option<String>,
    /// Sort key: the hand-set order when there is one, otherwise the id, so a
    /// board nobody has dragged still reads in id order.
    rank: f64,
}

struct RepoBoard {
    name: String,
    board: Board,
    lanes: Vec<Vec<Card>>,
    visible: bool,
}

pub struct BoardApp {
    repos: Vec<RepoBoard>,
    status: Option<String>,
}

impl BoardApp {
    pub fn load() -> Result<Self> {
        let current = Board::locate().ok();
        let registry = Registry::load();
        let entries = registry.boards(current.as_ref());

        let mut repos = Vec::new();
        for entry in entries {
            let board = entry.board();
            if !entry.has_board() {
                continue;
            }
            repos.push(RepoBoard {
                name: board.name(),
                lanes: read_lanes(&board)?,
                board,
                visible: true,
            });
        }
        if repos.is_empty() {
            return Err(anyhow!(
                "no boards found — run `amd init` here, or `amd repos add` to register one"
            ));
        }
        Ok(Self {
            repos,
            status: None,
        })
    }

    fn reload(&mut self) {
        for repo in &mut self.repos {
            match read_lanes(&repo.board) {
                Ok(lanes) => repo.lanes = lanes,
                Err(err) => self.status = Some(format!("{err}")),
            }
        }
    }

    /// Apply a drop: move between columns when the status changed, and always
    /// re-rank so the card keeps the position it was dropped at.
    fn apply(&mut self, drop: Drop) {
        let Drop::Land {
            path,
            repo,
            column,
            index,
        } = drop;

        let Some(target) = self.repos.get(repo) else {
            return;
        };
        // Rank between the neighbours at the drop point, ignoring the card
        // itself so dragging within a lane doesn't measure against its old
        // position.
        let lane: Vec<f64> = target.lanes[column as usize]
            .iter()
            .filter(|card| card.task.path != path)
            .map(|card| card.rank)
            .collect();
        let index = index.min(lane.len());
        let rank = match (
            index.checked_sub(1).and_then(|i| lane.get(i)),
            lane.get(index),
        ) {
            (Some(before), Some(after)) => (before + after) / 2.0,
            (Some(before), None) => before + 1.0,
            (None, Some(after)) => after - 1.0,
            (None, None) => 0.0,
        };

        let outcome = (|| -> Result<()> {
            let task = Task::from_path(&path, column)
                .ok_or_else(|| anyhow!("{} is not a task", path.display()))?;
            // Find it in whichever column it currently sits in.
            let found = target.board.find(&task.stem)?;
            if found.column != column {
                target.board.move_task(&found, column)?;
            }
            // Re-read: moving renamed the file.
            let moved = target.board.find(&task.stem)?;
            moved.set_order(rank)
        })();

        match outcome {
            Ok(()) => self.status = None,
            Err(err) => self.status = Some(format!("{err}")),
        }
        self.reload();
    }
}

fn read_lanes(board: &Board) -> Result<Vec<Vec<Card>>> {
    let mut lanes = Vec::new();
    for column in Column::ALL {
        let mut cards: Vec<Card> = board
            .tasks_in(column)?
            .into_iter()
            .map(|task| {
                let rank = task
                    .order()
                    .unwrap_or_else(|| task.id.map(f64::from).unwrap_or(f64::MAX));
                Card {
                    title: task.title(),
                    id: task.id_display(),
                    assignee: task.assignee(),
                    rank,
                    task,
                }
            })
            .collect();
        cards.sort_by(|a, b| {
            a.rank
                .partial_cmp(&b.rank)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        lanes.push(cards);
    }
    Ok(lanes)
}

#[cfg(feature = "gui")]
impl eframe::App for BoardApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut pending: Option<Drop> = None;
        let mut reload = false;

        egui::Panel::top(egui::Id::new("repos")).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("agile-md");
                ui.separator();
                for repo in &mut self.repos {
                    ui.checkbox(&mut repo.visible, &repo.name);
                }
                ui.separator();
                reload = ui.button("Reload").clicked();
            });
            if let Some(status) = &self.status {
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), status);
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                for (repo_index, repo) in self.repos.iter().enumerate() {
                    if !repo.visible {
                        continue;
                    }
                    if self.repos.iter().filter(|r| r.visible).count() > 1 {
                        ui.label(egui::RichText::new(&repo.name).strong());
                    }
                    for column in Column::ALL {
                        if let Some(drop) = lane_ui(ui, repo, repo_index, column) {
                            pending = Some(drop);
                        }
                    }
                    ui.add_space(12.0);
                }
            });
        });

        if let Some(drop) = pending {
            self.apply(drop);
        } else if reload {
            self.reload();
        }
    }
}

/// One status lane: a header, then its cards left to right, with a drop slot
/// before each card and one at the end.
#[cfg(feature = "gui")]
fn lane_ui(ui: &mut egui::Ui, repo: &RepoBoard, repo_index: usize, column: Column) -> Option<Drop> {
    let mut result = None;
    let cards = &repo.lanes[column as usize];

    ui.label(
        egui::RichText::new(column.as_str().to_uppercase())
            .monospace()
            .strong(),
    );

    let frame = egui::Frame::default()
        .inner_margin(6.0)
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(4.0);

    let (_, payload) = ui.dnd_drop_zone::<Dragged, ()>(frame, |ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(110.0);
            for (index, card) in cards.iter().enumerate() {
                if let Some(drop) = slot(ui, repo_index, column, index) {
                    result = Some(drop);
                }
                card_ui(ui, card, repo_index);
            }
            if let Some(drop) = slot(ui, repo_index, column, cards.len()) {
                result = Some(drop);
            }
            if cards.is_empty() {
                ui.weak("(empty)");
            }
        });
    });

    // Dropped on the lane but not on a slot: append.
    if let Some(dragged) = payload
        && result.is_none()
        && dragged.repo == repo_index
    {
        result = Some(Drop::Land {
            path: dragged.path.clone(),
            repo: repo_index,
            column,
            index: cards.len(),
        });
    }
    result
}

/// A thin gap between cards that accepts a drop, giving exact placement
/// without measuring pointer geometry.
#[cfg(feature = "gui")]
fn slot(ui: &mut egui::Ui, repo_index: usize, column: Column, index: usize) -> Option<Drop> {
    let frame = egui::Frame::default().inner_margin(2.0);
    let (_, payload) = ui.dnd_drop_zone::<Dragged, ()>(frame, |ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(6.0, 100.0), egui::Sense::hover());
        if ui.is_rect_visible(rect) && egui::DragAndDrop::has_any_payload(ui.ctx()) {
            ui.painter()
                .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
        }
    });
    payload.and_then(|dragged| {
        // A ticket carries its repository's ids and branch names with it, so
        // dropping one on another board would need a new id, not a rename.
        (dragged.repo == repo_index).then(|| Drop::Land {
            path: dragged.path.clone(),
            repo: repo_index,
            column,
            index,
        })
    })
}

/// A ticket: a portrait card, so a lane holds many across the width.
#[cfg(feature = "gui")]
fn card_ui(ui: &mut egui::Ui, card: &Card, repo_index: usize) {
    let id = egui::Id::new(("card", &card.task.path));
    let payload = Dragged {
        path: card.task.path.clone(),
        repo: repo_index,
    };
    ui.dnd_drag_source(id, payload, |ui| {
        egui::Frame::default()
            .inner_margin(8.0)
            .fill(ui.visuals().panel_fill)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.set_width(150.0);
                ui.set_min_height(94.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&card.id).monospace().weak());
                    ui.label(egui::RichText::new(&card.title).strong());
                    if let Some(who) = &card.assignee {
                        ui.weak(who);
                    }
                });
            });
    });
}

#[cfg(feature = "gui")]
pub fn run() -> Result<()> {
    let app = BoardApp::load()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "agile-md",
        options,
        Box::new(|_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    )
    .map_err(|err| anyhow!("{err}"))
}
