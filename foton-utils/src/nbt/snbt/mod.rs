mod error;
mod number;
mod parser;
#[cfg(feature = "codec-test-support")]
pub use parser::compound_work;
mod writer;

pub use error::{SnbtError, SnbtErrorKind, SnbtNumberType};
pub use parser::{
    parse_snbt, parse_snbt_argument, parse_snbt_compound, parse_snbt_compound_argument,
};
pub use writer::to_canonical_snbt;

#[cfg(test)]
mod tests;

mod bounded_writer;
#[cfg(feature = "codec-test-support")]
pub use bounded_writer::sort_work;
pub use bounded_writer::write_compound_snbt_nbt;
