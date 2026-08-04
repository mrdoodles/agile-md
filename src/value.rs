//! Argument parsers that know their own possible values.
//!
//! The point is completion: a parser that advertises its values makes
//! `amd new --type <TAB>` offer the change types instead of falling back to
//! filenames, and the same list turns up in `--help`. Validation stays ours, so
//! the errors keep saying how to change the list.

use std::ffi::OsStr;

use clap::builder::{PossibleValue, TypedValueParser};
use clap::error::ErrorKind;
use clap::{Arg, Command, Error};

use crate::{branch, templates};

/// The `--type` label: one of the board's change types.
#[derive(Clone, Copy, Debug)]
pub struct ChangeType;

impl TypedValueParser for ChangeType {
    type Value = String;

    fn parse_ref(
        &self,
        _cmd: &Command,
        _arg: Option<&Arg>,
        value: &OsStr,
    ) -> Result<String, Error> {
        let value = value.to_string_lossy().into_owned();
        // Our own message: it names the offending value and points at AMD_TYPES.
        branch::validate_type(&value)
            .map_err(|err| Error::raw(ErrorKind::InvalidValue, format!("{err}\n")))?;
        Ok(value)
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            branch::types().into_iter().map(PossibleValue::new),
        ))
    }
}

/// The ticket type (`-T`): suggests the built-in ones but accepts any name,
/// since a board can add its own templates.
#[derive(Clone, Copy, Debug)]
pub struct TicketType;

impl TypedValueParser for TicketType {
    type Value = String;

    fn parse_ref(
        &self,
        _cmd: &Command,
        _arg: Option<&Arg>,
        value: &OsStr,
    ) -> Result<String, Error> {
        Ok(value.to_string_lossy().into_owned())
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            templates::builtin_ticket_types()
                .into_iter()
                .map(PossibleValue::new),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values<P: TypedValueParser>(parser: &P) -> Vec<String> {
        parser
            .possible_values()
            .map(|values| values.map(|value| value.get_name().to_string()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn change_types_offer_the_conventional_list() {
        let offered = values(&ChangeType);
        assert!(offered.contains(&"feat".to_string()), "{offered:?}");
        assert!(offered.contains(&"revert".to_string()), "{offered:?}");
    }

    #[test]
    fn change_types_are_validated_with_our_message() {
        let err = ChangeType
            .parse_ref(&Command::new("amd"), None, OsStr::new("feature"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown type 'feature'"), "{err}");
        assert!(err.contains("AMD_TYPES"), "{err}");
    }

    #[test]
    fn ticket_types_suggest_the_built_ins_but_accept_a_board_template() {
        let offered = values(&TicketType);
        assert_eq!(offered, ["development", "admin"]);
        let parsed = TicketType
            .parse_ref(&Command::new("amd"), None, OsStr::new("spike"))
            .unwrap();
        assert_eq!(parsed, "spike");
    }
}
