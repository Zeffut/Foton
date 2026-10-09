//! Command arguments of `/scoreboard`: criteria, display slot, operation, style.

use foton_protocol::packets::game::{DisplaySlot, TextStyle};
use foton_utils::{nbt::parse_snbt_argument, translations};
use simdnbt::owned::NbtTag;
use text_components::TextComponent;

use crate::command::brigadier::{
    CommandSyntaxError, CommandSyntaxErrorKind, StringReader, SuggestionsBuilder,
};
use crate::scoreboard::ObjectiveCriteria;

/// Vanilla parity: `ObjectiveCriteriaArgument.parse`, which takes everything
/// up to the next space and fails at its start.
pub(super) fn parse_objective_criteria(
    reader: &mut StringReader<'_>,
) -> Result<ObjectiveCriteria, CommandSyntaxError> {
    let start = reader.checkpoint();
    let begin = reader.read_so_far().len();
    while reader.can_read() && reader.peek() != Some(' ') {
        reader.skip();
    }
    let raw = reader.input()[begin..reader.read_so_far().len()].to_owned();
    ObjectiveCriteria::by_name(&raw).ok_or_else(|| {
        reader.restore(start);
        reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(
            translations::ARGUMENT_CRITERIA_INVALID
                .message([raw])
                .component(),
        )))
    })
}

pub(super) fn suggest_objective_criteria(builder: &mut SuggestionsBuilder<'_>) {
    let prefix = builder.remaining().to_lowercase();
    for name in ObjectiveCriteria::suggestions() {
        if name.to_lowercase().starts_with(&prefix) {
            builder.suggest(name);
        }
    }
}

/// Vanilla parity: `ScoreboardSlotArgument.parse`.
pub(super) fn parse_display_slot(
    reader: &mut StringReader<'_>,
) -> Result<DisplaySlot, CommandSyntaxError> {
    let name = reader.read_unquoted_string().to_owned();
    DisplaySlot::by_name(&name).ok_or_else(|| {
        reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(
            translations::ARGUMENT_SCOREBOARD_DISPLAY_SLOT_INVALID
                .message([name])
                .component(),
        )))
    })
}

pub(super) fn suggest_display_slot(builder: &mut SuggestionsBuilder<'_>) {
    let prefix = builder.remaining().to_lowercase();
    for slot in DisplaySlot::VALUES {
        if slot.serialized_name().starts_with(&prefix) {
            builder.suggest(slot.serialized_name());
        }
    }
}

/// An `OperationArgument.Operation`: how `/scoreboard players operation`
/// combines the target's score with the source's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScoreOperation {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Min,
    Max,
    Swap,
}

/// `OperationArgument.ERROR_DIVIDE_BY_ZERO`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DivideByZero;

impl ScoreOperation {
    const SYMBOLS: [(&'static str, Self); 9] = [
        ("=", Self::Assign),
        ("+=", Self::Add),
        ("-=", Self::Subtract),
        ("*=", Self::Multiply),
        ("/=", Self::Divide),
        ("%=", Self::Modulo),
        ("<", Self::Min),
        (">", Self::Max),
        ("><", Self::Swap),
    ];

    /// Returns the target's and the source's scores after the operation.
    ///
    /// Vanilla parity: `Operation.apply`. Arithmetic wraps like Java's `int`,
    /// division and modulo are the flooring `Math.floorDiv` and
    /// `Math.floorMod`, and both refuse a zero divisor.
    pub(crate) fn apply(self, target: i32, source: i32) -> Result<(i32, i32), DivideByZero> {
        Ok(match self {
            Self::Assign => (source, source),
            Self::Add => (target.wrapping_add(source), source),
            Self::Subtract => (target.wrapping_sub(source), source),
            Self::Multiply => (target.wrapping_mul(source), source),
            Self::Divide => (floor_div(target, source).ok_or(DivideByZero)?, source),
            Self::Modulo => (floor_mod(target, source).ok_or(DivideByZero)?, source),
            Self::Min => (target.min(source), source),
            Self::Max => (target.max(source), source),
            Self::Swap => (source, target),
        })
    }
}

const fn floor_div(dividend: i32, divisor: i32) -> Option<i32> {
    if divisor == 0 {
        return None;
    }
    let quotient = dividend.wrapping_div(divisor);
    let inexact_negative = (dividend ^ divisor) < 0 && quotient.wrapping_mul(divisor) != dividend;
    Some(if inexact_negative {
        quotient - 1
    } else {
        quotient
    })
}

fn floor_mod(dividend: i32, divisor: i32) -> Option<i32> {
    let quotient = floor_div(dividend, divisor)?;
    Some(dividend.wrapping_sub(quotient.wrapping_mul(divisor)))
}

/// Vanilla parity: `OperationArgument.parse`.
pub(super) fn parse_operation(
    reader: &mut StringReader<'_>,
) -> Result<ScoreOperation, CommandSyntaxError> {
    if !reader.can_read() {
        return Err(reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(
            TextComponent::from(&translations::ARGUMENTS_OPERATION_INVALID),
        ))));
    }
    let begin = reader.read_so_far().len();
    while reader.can_read() && reader.peek() != Some(' ') {
        reader.skip();
    }
    let raw = &reader.input()[begin..reader.read_so_far().len()];
    ScoreOperation::SYMBOLS
        .iter()
        .find(|(symbol, _)| *symbol == raw)
        .map(|&(_, operation)| operation)
        .ok_or_else(|| {
            CommandSyntaxError::dynamic(TextComponent::from(
                &translations::ARGUMENTS_OPERATION_INVALID,
            ))
        })
}

pub(super) fn suggest_operation(builder: &mut SuggestionsBuilder<'_>) {
    let prefix = builder.remaining();
    for (symbol, _) in ScoreOperation::SYMBOLS {
        if symbol.starts_with(prefix) {
            builder.suggest(symbol);
        }
    }
}

/// Vanilla parity: `StyleArgument`, an SNBT compound decoded with
/// `Style.Serializer.CODEC`. Its fields are the style fields of a component,
/// so the component parser reads them; the content it requires is supplied.
pub(super) fn parse_style(reader: &mut StringReader<'_>) -> Result<TextStyle, CommandSyntaxError> {
    let start = reader.checkpoint();
    let (tag, consumed) = parse_snbt_argument(reader.remaining()).map_err(|error| {
        reader.advance_bytes(error.cursor());
        invalid_style(reader, error.component())
    })?;
    if !reader.advance_bytes(consumed) {
        return Err(invalid_style(reader, "Invalid style cursor"));
    }
    let NbtTag::Compound(mut compound) = tag else {
        reader.restore(start);
        return Err(invalid_style(reader, "Not a compound"));
    };
    if compound.get("text").is_none() {
        compound.insert("text", "");
    }
    let component = TextComponent::try_from_nbt(&NbtTag::Compound(compound)).map_err(|error| {
        reader.restore(start);
        invalid_style(reader, error.to_string())
    })?;
    Ok(TextStyle::of(&component))
}

fn invalid_style(
    reader: &StringReader<'_>,
    message: impl Into<TextComponent>,
) -> CommandSyntaxError {
    reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(
        translations::ARGUMENT_STYLE_INVALID
            .message([message.into()])
            .component(),
    )))
}

#[cfg(test)]
mod tests {
    use super::{DivideByZero, ScoreOperation};

    /// Java's `Math.floorDiv`/`floorMod` round toward negative infinity, not
    /// toward zero, so `-7 /= 2` is -4 and `-7 %= 2` is 1.
    #[test]
    fn division_and_modulo_floor() {
        assert_eq!(ScoreOperation::Divide.apply(-7, 2), Ok((-4, 2)));
        assert_eq!(ScoreOperation::Modulo.apply(-7, 2), Ok((1, 2)));
        assert_eq!(ScoreOperation::Modulo.apply(7, -2), Ok((-1, -2)));
        assert_eq!(
            ScoreOperation::Divide.apply(i32::MIN, -1),
            Ok((i32::MIN, -1))
        );
        assert_eq!(ScoreOperation::Divide.apply(1, 0), Err(DivideByZero));
        assert_eq!(ScoreOperation::Modulo.apply(1, 0), Err(DivideByZero));
    }

    #[test]
    fn arithmetic_wraps_and_swap_exchanges() {
        assert_eq!(ScoreOperation::Add.apply(i32::MAX, 1), Ok((i32::MIN, 1)));
        assert_eq!(ScoreOperation::Swap.apply(3, 9), Ok((9, 3)));
        assert_eq!(ScoreOperation::Min.apply(3, 9), Ok((3, 9)));
        assert_eq!(ScoreOperation::Max.apply(3, 9), Ok((9, 9)));
    }
}
