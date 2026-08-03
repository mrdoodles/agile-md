//! Interactive prompts, built on `inquire`.
//!
//! Everything here is optional: `amd` stays fully scriptable, so each prompt
//! has a flag or argument that skips it, and nothing prompts unless both stdin
//! and stdout are a terminal. `--no-input` (or `AMD_NO_INPUT=1`) turns the
//! prompts off entirely, so CI fails with a clear message instead of blocking.

use std::env;
use std::fmt;
use std::io::{self, IsTerminal};
use std::sync::OnceLock;

use anyhow::{Result, bail};
use inquire::autocompletion::{Autocomplete, Replacement};
use inquire::validator::Validation;
use inquire::{Confirm, CustomUserError, InquireError, Select, Text};

use crate::branch;
use crate::task::Task;

static NO_INPUT: OnceLock<bool> = OnceLock::new();

/// Record the `--no-input` flag. Called once, from `main`.
pub fn set_no_input(no_input: bool) {
    let _ = NO_INPUT.set(no_input);
}

/// Can we prompt? Requires a terminal on both ends and no opt-out.
pub fn available() -> bool {
    if *NO_INPUT.get().unwrap_or(&false) {
        return false;
    }
    if env::var("AMD_NO_INPUT").as_deref() == Ok("1") {
        return false;
    }
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

/// A free-text answer. `default` pre-fills the line for editing.
pub fn text(message: &str, default: Option<&str>, help: Option<&str>) -> Result<String> {
    let mut prompt = Text::new(message);
    if let Some(default) = default {
        prompt = prompt.with_initial_value(default);
    }
    if let Some(help) = help {
        prompt = prompt.with_help_message(help);
    }
    convert(prompt.prompt())
}

/// The task title. Validated as you type against the branch-name rules, since
/// the title becomes `<type>/<slug>` — better to catch it here than to leave a
/// ticket that can never start.
pub fn title(kind: &str, default: Option<&str>) -> Result<String> {
    let help = format!("becomes the branch {kind}/<title>");
    let kind = kind.to_string();
    let mut prompt =
        Text::new("Title:")
            .with_help_message(&help)
            .with_validator(move |input: &str| {
                Ok(match branch::validate_title(&kind, input) {
                    Ok(()) => Validation::Valid,
                    Err(err) => Validation::Invalid(err.to_string().into()),
                })
            });
    if let Some(default) = default {
        prompt = prompt.with_initial_value(default);
    }
    Ok(convert(prompt.prompt())?.trim().to_string())
}

/// A single label, completing against the ones already on the board. Empty
/// means "no label".
pub fn label(message: &str, default: &str, known: Vec<String>, help: &str) -> Result<String> {
    let answer = convert(
        Text::new(message)
            .with_initial_value(default)
            .with_help_message(help)
            .with_autocomplete(Completer::single(known))
            .prompt(),
    )?;
    Ok(answer.trim().to_string())
}

/// Pick one of `options`, starting on `default` when it's among them.
pub fn select<T: fmt::Display + Eq>(message: &str, options: Vec<T>, default: &T) -> Result<T> {
    let start = options.iter().position(|option| option == default);
    let mut prompt = Select::new(message, options);
    if let Some(start) = start {
        prompt = prompt.with_starting_cursor(start);
    }
    convert(prompt.prompt())
}

/// A comma-separated tag list, completing against the tags already on the
/// board so a board doesn't accumulate `docs`, `doc` and `documentation`.
pub fn tags(default: &str, known: Vec<String>) -> Result<Vec<String>> {
    let help = if known.is_empty() {
        "comma separated".to_string()
    } else {
        format!("comma separated; tab completes: {}", known.join(", "))
    };
    let answer = convert(
        Text::new("Tags:")
            .with_initial_value(default)
            .with_help_message(&help)
            .with_autocomplete(Completer::multi(known))
            .prompt(),
    )?;
    Ok(answer
        .split(',')
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect())
}

/// Completes a label against the ones already on the board. In `multi` mode it
/// completes the value after the last comma, leaving earlier ones alone.
#[derive(Clone)]
struct Completer {
    known: Vec<String>,
    multi: bool,
}

impl Completer {
    fn single(known: Vec<String>) -> Self {
        Self {
            known,
            multi: false,
        }
    }

    fn multi(known: Vec<String>) -> Self {
        Self { known, multi: true }
    }

    fn split(&self, input: &str) -> (String, String) {
        if self.multi {
            split_last_value(input)
        } else {
            (String::new(), input.to_string())
        }
    }

    fn matches(&self, partial: &str) -> Vec<&String> {
        let partial = partial.to_lowercase();
        self.known
            .iter()
            .filter(|value| value.to_lowercase().starts_with(&partial))
            .collect()
    }
}

impl Autocomplete for Completer {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let (prefix, partial) = self.split(input);
        Ok(self
            .matches(&partial)
            .into_iter()
            .map(|value| format!("{prefix}{value}"))
            .collect())
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        if let Some(suggestion) = highlighted {
            return Ok(Replacement::Some(suggestion));
        }
        let (prefix, partial) = self.split(input);
        Ok(self
            .matches(&partial)
            .first()
            .map(|value| format!("{prefix}{value}")))
    }
}

/// `"docs, rel"` -> `("docs, ", "rel")`.
fn split_last_value(input: &str) -> (String, String) {
    match input.rfind(',') {
        Some(comma) => (
            format!("{} ", &input[..=comma]),
            input[comma + 1..].trim_start().to_string(),
        ),
        None => (String::new(), input.to_string()),
    }
}

/// A yes/no question, defaulting to no.
pub fn confirm(message: &str) -> Result<bool> {
    convert(Confirm::new(message).with_default(false).prompt())
}

/// Pick a task from a list, shown the way the board shows them.
pub fn select_task(message: &str, tasks: Vec<Task>) -> Result<Task> {
    if tasks.is_empty() {
        bail!("no tasks to choose from");
    }
    let choices: Vec<Choice> = tasks.into_iter().map(Choice).collect();
    Ok(convert(Select::new(message, choices).prompt())?.0)
}

/// A task rendered as one line of the picker.
struct Choice(Task);

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} ({}/)",
            self.0.id_display(),
            self.0.title(),
            self.0.column
        )
    }
}

/// Map inquire's cancellations onto a plain "cancelled" error, so Esc and
/// Ctrl-C read as a decision rather than a crash.
fn convert<T>(result: Result<T, InquireError>) -> Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            bail!("cancelled")
        }
        Err(InquireError::NotTTY) => bail!("not a terminal — pass the values as arguments"),
        Err(err) => bail!(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn autocomplete() -> Completer {
        Completer::multi(vec!["docs".into(), "release".into(), "regression".into()])
    }

    #[test]
    fn split_last_value_keeps_the_tags_already_typed() {
        assert_eq!(
            split_last_value("docs, rel"),
            ("docs, ".to_string(), "rel".to_string())
        );
        assert_eq!(split_last_value("rel"), (String::new(), "rel".to_string()));
        assert_eq!(split_last_value(""), (String::new(), String::new()));
    }

    #[test]
    fn suggestions_complete_only_the_last_tag() {
        let mut completer = autocomplete();
        assert_eq!(
            completer.get_suggestions("docs, re").unwrap(),
            ["docs, release", "docs, regression"]
        );
    }

    #[test]
    fn an_empty_partial_offers_every_known_tag() {
        let mut completer = autocomplete();
        assert_eq!(completer.get_suggestions("").unwrap().len(), 3);
    }

    #[test]
    fn completion_falls_back_to_the_first_match() {
        let mut completer = autocomplete();
        assert_eq!(
            completer.get_completion("reg", None).unwrap(),
            Replacement::Some("regression".to_string())
        );
        assert_eq!(
            completer.get_completion("nope", None).unwrap(),
            Replacement::None
        );
    }
}
