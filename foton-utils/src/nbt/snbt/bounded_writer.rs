//! Streaming canonical SNBT for the string-valued block predicate codec.
use super::writer::{java_double_string, java_float_string};
use crate::serial::nbt_stream::{LimitedWriter, NbtWrite};
use simdnbt::{
    Mutf8Str,
    owned::{NbtCompound, NbtList, NbtTag},
};
use std::{
    fmt::{self, Write as _},
    io,
};

/// Test-support observations of canonical SNBT key comparisons.
#[cfg(feature = "codec-test-support")]
pub mod sort_work {
    use std::cell::Cell;

    thread_local! { static COMPARISONS: Cell<usize> = const { Cell::new(0) }; }

    pub(super) fn compare() {
        COMPARISONS.set(COMPARISONS.get() + 1);
    }

    /// Counts key comparisons performed by an actual bounded writer route.
    pub fn measure(operation: impl FnOnce()) -> usize {
        COMPARISONS.set(0);
        operation();
        COMPARISONS.get()
    }
}

struct UtfWriter<'a> {
    writer: &'a mut dyn NbtWrite,
    error: Option<io::Error>,
    length: usize,
}
impl fmt::Write for UtfWriter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for unit in value.encode_utf16() {
            let mut bytes = [0; 3];
            let length = if (1..=127).contains(&unit) {
                bytes[0] = unit as u8;
                1
            } else if unit <= 2047 {
                bytes[0] = 0xc0 | (unit >> 6) as u8;
                bytes[1] = 0x80 | (unit & 63) as u8;
                2
            } else {
                bytes[0] = 0xe0 | (unit >> 12) as u8;
                bytes[1] = 0x80 | ((unit >> 6) & 63) as u8;
                bytes[2] = 0x80 | (unit & 63) as u8;
                3
            };
            self.length += length;
            if self.length > u16::MAX as usize {
                return Err(fmt::Error);
            }
            if let Err(error) = self.writer.write_all(&bytes[..length]) {
                self.error = Some(error);
                return Err(fmt::Error);
            }
        }
        Ok(())
    }
}
/// Writes a compound's canonical SNBT as an NBT string payload without cloning the tree.
pub fn write_compound_snbt_nbt(value: &NbtCompound, writer: &mut dyn NbtWrite) -> io::Result<()> {
    writer.ensure_remaining(2)?;
    let mut sink = io::sink();
    let mut limited = LimitedWriter::new(&mut sink, writer.remaining() - 2);
    let mut count = UtfWriter {
        writer: &mut limited,
        length: 0,
        error: None,
    };
    if compound(value, &mut count, 0, &mut writer.scratch_remaining()).is_err() {
        return Err(count
            .error
            .unwrap_or_else(|| io::Error::other("SNBT string exceeds codec bounds")));
    }
    writer.write_all(&(count.length as u16).to_be_bytes())?;
    let mut scratch = writer.scratch_remaining();
    let mut output = UtfWriter {
        writer,
        length: 0,
        error: None,
    };
    if compound(value, &mut output, 0, &mut scratch).is_err() {
        return Err(output
            .error
            .unwrap_or_else(|| io::Error::other("SNBT output failed")));
    }
    Ok(())
}
impl UtfWriter<'_> {
    fn ensure_remaining(&mut self, minimum: usize) -> fmt::Result {
        if let Err(error) = self.writer.ensure_remaining(minimum) {
            self.error = Some(error);
            return Err(fmt::Error);
        }
        Ok(())
    }
}
fn string(value: &Mutf8Str, output: &mut UtfWriter<'_>) -> Result<String, fmt::Error> {
    output.ensure_remaining(value.len())?;
    if value.len() > u16::MAX as usize {
        return Err(fmt::Error);
    }
    value.to_owned().try_into_string().map_err(|_| fmt::Error)
}
fn quote(value: &str, output: &mut UtfWriter<'_>) -> fmt::Result {
    output.ensure_remaining(value.len().saturating_add(2))?;
    let quote = value
        .chars()
        .find_map(|c| match c {
            '"' => Some('\''),
            '\'' => Some('"'),
            _ => None,
        })
        .unwrap_or('"');
    output.write_char(quote)?;
    for c in value.chars() {
        match c {
            '\\' => output.write_str("\\\\")?,
            c if c == quote => {
                output.write_char('\\')?;
                output.write_char(c)?;
            }
            '\u{0008}' => output.write_str("\\b")?,
            '\t' => output.write_str("\\t")?,
            '\n' => output.write_str("\\n")?,
            '\u{000c}' => output.write_str("\\f")?,
            '\r' => output.write_str("\\r")?,
            c if c < ' ' => write!(output, "\\x{:02x}", u32::from(c))?,
            c => output.write_char(c)?,
        }
    }
    output.write_char(quote)
}
fn key(value: &str, output: &mut UtfWriter<'_>) -> fmt::Result {
    let mut chars = value.chars();
    let simple = !value.eq_ignore_ascii_case("true")
        && !value.eq_ignore_ascii_case("false")
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, '.' | '_'))
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    if simple {
        output.write_str(value)
    } else {
        quote(value, output)
    }
}
fn compound(
    value: &NbtCompound,
    output: &mut UtfWriter<'_>,
    depth: usize,
    scratch: &mut usize,
) -> fmt::Result {
    if depth >= 512 || value.len() > u16::MAX as usize / 4 {
        return Err(fmt::Error);
    }
    // Reserve the borrowed-entry index and decoded keys once. This allowance is
    // shared with children while their parents' sorting buffers are still live.
    let punctuation = if value.is_empty() { 2 } else { 1 };
    output.ensure_remaining(value.len().saturating_mul(4).saturating_add(punctuation))?;
    let mut minimum = punctuation;
    let mut required = value.len().saturating_mul(size_of::<(String, &NbtTag)>());
    for (key, _) in value.iter() {
        minimum = minimum.saturating_add(key.len()).saturating_add(3);
        output.ensure_remaining(minimum)?;
        if key.len() > u16::MAX as usize {
            return Err(fmt::Error);
        }
        required = required.saturating_add(key.len().saturating_mul(3));
    }
    *scratch = scratch.checked_sub(required).ok_or(fmt::Error)?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(value.len())
        .map_err(|_| fmt::Error)?;
    for (name, value) in value.iter() {
        entries.push((string(name, output)?, value));
    }
    entries.sort_unstable_by(|(a, _), (b, _)| {
        #[cfg(feature = "codec-test-support")]
        sort_work::compare();
        a.encode_utf16().cmp(b.encode_utf16())
    });
    output.write_char('{')?;
    for (index, (name, value)) in entries.iter().enumerate() {
        if index > 0 {
            output.write_char(',')?;
        }
        key(name, output)?;
        output.write_char(':')?;
        tag(value, output, depth + 1, scratch)?;
    }
    drop(entries);
    *scratch += required;
    output.write_char('}')
}
fn tag(
    value: &NbtTag,
    output: &mut UtfWriter<'_>,
    depth: usize,
    scratch: &mut usize,
) -> fmt::Result {
    if depth > 512 {
        return Err(fmt::Error);
    }
    match value {
        NbtTag::Byte(v) => write!(output, "{v}b"),
        NbtTag::Short(v) => write!(output, "{v}s"),
        NbtTag::Int(v) => write!(output, "{v}"),
        NbtTag::Long(v) => write!(output, "{v}L"),
        NbtTag::Float(v) => write!(output, "{}f", java_float_string(*v)),
        NbtTag::Double(v) => write!(output, "{}d", java_double_string(*v)),
        NbtTag::String(v) => quote(&string(v, output)?, output),
        NbtTag::Compound(v) => compound(v, output, depth, scratch),
        NbtTag::List(v) => list(v, output, depth, scratch),
        NbtTag::ByteArray(v) => {
            output.write_str("[B;")?;
            for (i, v) in v.iter().enumerate() {
                if i > 0 {
                    output.write_char(',')?;
                }
                write!(output, "{}B", *v as i8)?;
            }
            output.write_char(']')
        }
        NbtTag::IntArray(v) => {
            output.write_str("[I;")?;
            for (i, v) in v.iter().enumerate() {
                if i > 0 {
                    output.write_char(',')?;
                }
                write!(output, "{v}")?;
            }
            output.write_char(']')
        }
        NbtTag::LongArray(v) => {
            output.write_str("[L;")?;
            for (i, v) in v.iter().enumerate() {
                if i > 0 {
                    output.write_char(',')?;
                }
                write!(output, "{v}L")?;
            }
            output.write_char(']')
        }
    }
}
fn list(
    value: &NbtList,
    output: &mut UtfWriter<'_>,
    depth: usize,
    scratch: &mut usize,
) -> fmt::Result {
    if depth >= 512 {
        return Err(fmt::Error);
    }
    output.write_char('[')?;
    macro_rules! values {
        ($values:expr, $value:ident, $body:expr) => {
            for (i, $value) in $values.iter().enumerate() {
                if i > 0 {
                    output.write_char(',')?;
                }
                $body?;
            }
        };
    }
    match value {
        NbtList::Empty => {}
        NbtList::Byte(v) => {
            values!(v, x, write!(output, "{x}b"));
        }
        NbtList::Short(v) => {
            values!(v, x, write!(output, "{x}s"));
        }
        NbtList::Int(v) => {
            values!(v, x, write!(output, "{x}"));
        }
        NbtList::Long(v) => {
            values!(v, x, write!(output, "{x}L"));
        }
        NbtList::Float(v) => {
            values!(v, x, write!(output, "{}f", java_float_string(*x)));
        }
        NbtList::Double(v) => {
            values!(v, x, write!(output, "{}d", java_double_string(*x)));
        }
        NbtList::String(v) => {
            values!(v, x, quote(&string(x, output)?, output));
        }
        NbtList::Compound(v) => {
            values!(
                v,
                x,
                if x.len() == 1 {
                    if let Some(tag_value) = x.get("") {
                        tag(tag_value, output, depth + 1, scratch)
                    } else {
                        compound(x, output, depth + 1, scratch)
                    }
                } else {
                    compound(x, output, depth + 1, scratch)
                }
            );
        }
        NbtList::List(v) => {
            values!(v, x, list(x, output, depth + 1, scratch));
        }
        NbtList::ByteArray(v) => {
            values!(v, x, {
                output.write_str("[B;")?;
                for (i, v) in x.iter().enumerate() {
                    if i > 0 {
                        output.write_char(',')?;
                    }
                    write!(output, "{}B", *v as i8)?;
                }
                output.write_char(']')
            });
        }
        NbtList::IntArray(v) => {
            values!(v, x, {
                output.write_str("[I;")?;
                for (i, v) in x.iter().enumerate() {
                    if i > 0 {
                        output.write_char(',')?;
                    }
                    write!(output, "{v}")?;
                }
                output.write_char(']')
            });
        }
        NbtList::LongArray(v) => {
            values!(v, x, {
                output.write_str("[L;")?;
                for (i, v) in x.iter().enumerate() {
                    if i > 0 {
                        output.write_char(',')?;
                    }
                    write!(output, "{v}L")?;
                }
                output.write_char(']')
            });
        }
    }
    output.write_char(']')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serial::text_stream;

    #[test]
    fn borrowed_snbt_preserves_java_order_escaping_numbers_and_wrapped_lists() {
        let sources = [
            r#"{z:1,a:'quote" and \'',"true":true,empty:"",unicode:"😀"}"#,
            r"{ints:[I;-1,0,2147483647],bytes:[B;-128B,127B],longs:[L;-1L,1L]}",
            r"{floats:[-0.0f,1.0e-20f,3.4028235e38f],doubles:[-0.0d,1.0e100d]}",
            r#"{list:[1,"two",{x:3},[4,5]],"𐀀":0,"":1,"\n":2}"#,
        ];
        for source in sources {
            let value = super::super::parse_snbt(source).expect("test SNBT");
            let compound = value.compound().expect("compound");
            let canonical = super::super::to_canonical_snbt(&value).expect("ordinary SNBT");
            let mut expected = Vec::new();
            text_stream::write_utf(
                &canonical,
                &mut LimitedWriter::new(&mut expected, usize::MAX),
            )
            .expect("ordinary UTF");
            let mut actual = Vec::new();
            write_compound_snbt_nbt(compound, &mut LimitedWriter::new(&mut actual, usize::MAX))
                .expect("borrowed SNBT");
            assert_eq!(actual, expected, "{source}");
        }
    }
}

#[cfg(test)]
mod brewing_rereview_tests {
    use super::*;
    #[test]
    fn brewing_rereview_snbt_exact_allowance_accepts_short_scalar_fields() {
        for source in ["{}", "{a:0}", "{a:0,b:1}", "{a:{b:1}}"] {
            let tag = super::super::parse_snbt(source).expect("SNBT");
            let compound = tag.compound().expect("compound");
            let canonical = super::super::to_canonical_snbt(&tag).expect("canonical");
            let cap = canonical.len() + 2;
            let mut bytes = Vec::new();
            write_compound_snbt_nbt(compound, &mut LimitedWriter::new(&mut bytes, cap))
                .expect("exact allowance");
            assert_eq!(&bytes[2..], canonical.as_bytes());
            assert!(
                write_compound_snbt_nbt(
                    compound,
                    &mut LimitedWriter::new(&mut io::sink(), cap - 1)
                )
                .is_err()
            );
        }
    }
}
