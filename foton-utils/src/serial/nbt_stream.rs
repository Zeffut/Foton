//! Safe NBT wire output over borrowed owned values, without a temporary vector.
use simdnbt::{
    Mutf8Str,
    owned::{NbtCompound, NbtList, NbtTag},
};
use std::io::{Error, ErrorKind, Result, Write, sink};
use std::ops::{Deref, DerefMut};

/// Test-only observations of actual bounded-writer forwarding work.
#[cfg(feature = "codec-test-support")]
pub mod writer_work {
    use std::cell::Cell;

    /// Calls entering a byte or metadata adapter on the measured thread.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Visits {
        /// Calls through a bounded byte adapter.
        pub writes: usize,
        /// Calls through a bounded metadata adapter.
        pub metadata: usize,
    }
    thread_local! {
        static WORK: Cell<Visits> = const { Cell::new(Visits { writes: 0, metadata: 0 }) };
    }
    pub(crate) fn write() {
        WORK.update(|v| Visits {
            writes: v.writes + 1,
            ..v
        });
    }
    pub(crate) fn metadata() {
        WORK.update(|v| Visits {
            metadata: v.metadata + 1,
            ..v
        });
    }
    /// Runs an actual codec route, counting all instrumented adapter visits.
    pub fn measure(operation: impl FnOnce()) -> Visits {
        WORK.set(Visits::default());
        operation();
        WORK.get()
    }
}

/// A byte-limited codec sink whose allowance also bounds preliminary work.
pub trait NbtWrite: Write {
    /// Bytes available to this codec and all its descendants.
    fn remaining(&self) -> usize;
    /// Exclusive container depth ceiling of the destination NBT reader.
    fn container_limit(&self) -> usize;
    /// Whether unordered compounds need canonical bridge ordering.
    fn canonical_compounds(&self) -> bool;
    /// Shared maximum live sorting workspace along this recursive path.
    fn scratch_remaining(&self) -> usize;
    /// Updates the shared reservation. Use `ScratchWriter` for scoped changes.
    fn set_scratch_remaining(&mut self, remaining: usize);
    /// Rejects an impossible minimum before inspecting attacker-owned contents.
    fn ensure_remaining(&self, minimum: usize) -> Result<()> {
        if minimum > self.remaining() {
            return Err(invalid("serialization budget exceeded"));
        }
        Ok(())
    }
}

/// A sink adapter enforcing a caller-supplied remaining wire budget.
pub struct LimitedWriter<'a> {
    writer: &'a mut dyn Write,
    remaining: usize,
    container_limit: usize,
    canonical_compounds: bool,
    scratch_remaining: usize,
}
impl<'a> LimitedWriter<'a> {
    /// Wraps a sink with its remaining byte budget.
    /// `usize::MAX` preserves ordinary compound storage order; finite caps canonicalize.
    pub fn new(writer: &'a mut dyn Write, remaining: usize) -> Self {
        Self {
            writer,
            remaining,
            container_limit: 512,
            canonical_compounds: remaining != usize::MAX,
            scratch_remaining: 384 * 1024,
        }
    }
    /// The persistent borrowed reader reserves one of its 512 stack slots.
    pub fn persistent(writer: &'a mut dyn Write, remaining: usize) -> Self {
        Self {
            writer,
            remaining,
            container_limit: 511,
            canonical_compounds: remaining != usize::MAX,
            scratch_remaining: 384 * 1024,
        }
    }
    /// Returns the number of bytes still available.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.remaining
    }
}
impl NbtWrite for LimitedWriter<'_> {
    fn remaining(&self) -> usize {
        #[cfg(feature = "codec-test-support")]
        writer_work::metadata();
        self.remaining
    }
    fn container_limit(&self) -> usize {
        #[cfg(feature = "codec-test-support")]
        writer_work::metadata();
        self.container_limit
    }
    fn canonical_compounds(&self) -> bool {
        #[cfg(feature = "codec-test-support")]
        writer_work::metadata();
        self.canonical_compounds
    }
    fn scratch_remaining(&self) -> usize {
        #[cfg(feature = "codec-test-support")]
        writer_work::metadata();
        self.scratch_remaining
    }
    fn set_scratch_remaining(&mut self, remaining: usize) {
        self.scratch_remaining = remaining;
    }
}
impl Write for LimitedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        #[cfg(feature = "codec-test-support")]
        writer_work::write();
        if bytes.len() > self.remaining {
            return Err(invalid("serialization budget exceeded"));
        }
        let written = self.writer.write(bytes)?;
        self.remaining -= written;
        Ok(written)
    }
    fn flush(&mut self) -> Result<()> {
        self.writer.flush()
    }
}
fn invalid(message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

/// Computes exact encoded size, stopping at the caller budget.
pub fn wire_size(tag: &NbtTag, maximum: usize) -> Result<usize> {
    let mut sink = sink();
    let mut writer = LimitedWriter::new(&mut sink, maximum);
    write_tag(tag, &mut writer, 0)?;
    Ok(maximum - writer.remaining())
}
/// Validates then streams a complete nameless NBT tag.
pub(crate) fn write_tag_bounded(
    tag: &NbtTag,
    maximum: usize,
    writer: &mut dyn Write,
) -> Result<()> {
    wire_size(tag, maximum)?;
    write_tag(tag, &mut LimitedWriter::new(writer, maximum), 0)
}
/// Validates then streams a borrowed compound including its tag ID.
pub fn write_compound_bounded(
    value: &NbtCompound,
    maximum: usize,
    writer: &mut dyn Write,
) -> Result<()> {
    let mut sink = sink();
    let mut checked = LimitedWriter::new(&mut sink, maximum);
    checked.write_all(&[10])?;
    write_compound(value, &mut checked, 0)?;
    writer.write_all(&[10])?;
    write_compound(
        value,
        &mut LimitedWriter::new(writer, maximum.saturating_sub(1)),
        0,
    )
}
/// Streams a tag including its ID, enforcing NBT nesting limits.
pub fn write_tag(tag: &NbtTag, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    writer.write_all(&[tag.id()])?;
    write_payload(tag, writer, depth)
}
fn check_depth(depth: usize) -> Result<()> {
    if depth > 512 {
        return Err(invalid("NBT nesting exceeds limit"));
    }
    Ok(())
}
/// Writes an already encoded MUTF-8 string with checked u16 length.
fn write_string(value: &Mutf8Str, writer: &mut dyn NbtWrite) -> Result<()> {
    let length =
        u16::try_from(value.len()).map_err(|_| invalid("NBT string exceeds u16 length"))?;
    writer.ensure_remaining(2 + value.len())?;
    if writer.canonical_compounds() && !valid_mutf8(value.as_bytes()) {
        return Err(invalid("Malformed NBT modified UTF-8"));
    }
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(value.as_bytes())
}
fn write_length(length: usize, writer: &mut dyn NbtWrite) -> Result<()> {
    let length = i32::try_from(length).map_err(|_| invalid("NBT array exceeds i32 length"))?;
    writer.write_all(&length.to_be_bytes())
}
// Java DataOutput canonicalizes NaNs with floatToIntBits/doubleToLongBits.
pub(super) const fn float_bits(value: f32) -> u32 {
    if value.is_nan() {
        0x7fc0_0000
    } else {
        value.to_bits()
    }
}
pub(super) const fn double_bits(value: f64) -> u64 {
    if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    }
}

/// Streams a tag payload without its ID or name.
pub(crate) fn write_payload(tag: &NbtTag, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    check_depth(depth)?;
    match tag {
        NbtTag::Byte(v) => writer.write_all(&v.to_be_bytes()),
        NbtTag::Short(v) => writer.write_all(&v.to_be_bytes()),
        NbtTag::Int(v) => writer.write_all(&v.to_be_bytes()),
        NbtTag::Long(v) => writer.write_all(&v.to_be_bytes()),
        NbtTag::Float(v) => writer.write_all(&float_bits(*v).to_be_bytes()),
        NbtTag::Double(v) => writer.write_all(&double_bits(*v).to_be_bytes()),
        NbtTag::String(v) => write_string(v, writer),
        NbtTag::ByteArray(v) => {
            write_length(v.len(), writer)?;
            writer.write_all(v)
        }
        NbtTag::IntArray(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                writer.write_all(&x.to_be_bytes())?;
            }
            Ok(())
        }
        NbtTag::LongArray(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                writer.write_all(&x.to_be_bytes())?;
            }
            Ok(())
        }
        NbtTag::Compound(v) => write_compound(v, writer, depth),
        NbtTag::List(v) => write_list(v, writer, depth),
    }
}
/// Streams named compound entries and their terminating end tag.
pub fn write_compound(value: &NbtCompound, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    write_compound_fields(value, writer, depth)?;
    writer.write_all(&[0])
}
/// Streams an unordered compound's fields, leaving the end tag to its enclosing codec.
pub fn write_compound_fields(
    value: &NbtCompound,
    writer: &mut dyn NbtWrite,
    depth: usize,
) -> Result<()> {
    if depth >= writer.container_limit() {
        return Err(invalid("NBT nesting exceeds limit"));
    }
    writer.ensure_remaining(value.len().saturating_mul(4).saturating_add(1))?;
    if !writer.canonical_compounds() {
        for (name, tag) in value.iter() {
            writer.write_all(&[tag.id()])?;
            write_string(name, writer)?;
            write_payload(tag, writer, depth + 1)?;
        }
        return Ok(());
    }
    let mut minimum = 1_usize;
    for (name, _) in value.iter() {
        minimum = minimum.saturating_add(name.len()).saturating_add(4);
        writer.ensure_remaining(minimum)?;
        if name.len() > u16::MAX as usize {
            return Err(invalid("NBT string exceeds u16 length"));
        }
    }
    let required = value
        .len()
        .saturating_mul(size_of::<(&Mutf8Str, &NbtTag)>());
    if required > writer.scratch_remaining() {
        return Err(invalid("NBT sorting workspace exceeds limit"));
    }
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(value.len())
        .map_err(Error::other)?;
    entries.extend(value.iter());
    entries.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    if entries
        .windows(2)
        .any(|pair| pair[0].0.as_bytes() == pair[1].0.as_bytes())
    {
        return Err(invalid("Duplicate NBT compound key"));
    }
    let mut writer = ScratchWriter::new(writer, required)?;
    for (name, tag) in entries {
        writer.write_all(&[tag.id()])?;
        write_string(name, &mut *writer)?;
        write_payload(tag, &mut *writer, depth + 1)?;
    }
    Ok(())
}
/// Reserves live sorting workspace shared with every descendant writer.
pub struct ScratchWriter<'a> {
    writer: &'a mut dyn NbtWrite,
    previous: usize,
}
impl<'a> ScratchWriter<'a> {
    /// Charges an already bounded index for the duration of its traversal.
    pub fn new(writer: &'a mut dyn NbtWrite, used: usize) -> Result<Self> {
        let previous = writer.scratch_remaining();
        let remaining = previous
            .checked_sub(used)
            .ok_or_else(|| invalid("NBT sorting workspace exceeds limit"))?;
        writer.set_scratch_remaining(remaining);
        Ok(Self { writer, previous })
    }
}
impl Drop for ScratchWriter<'_> {
    fn drop(&mut self) {
        self.writer.set_scratch_remaining(self.previous);
    }
}
impl Write for ScratchWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        #[cfg(feature = "codec-test-support")]
        writer_work::write();
        self.writer.write(bytes)
    }
    fn flush(&mut self) -> Result<()> {
        self.writer.flush()
    }
}
impl<'a> Deref for ScratchWriter<'a> {
    type Target = dyn NbtWrite + 'a;
    fn deref(&self) -> &Self::Target {
        self.writer
    }
}
impl DerefMut for ScratchWriter<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.writer
    }
}
fn write_list(list: &NbtList, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    if depth >= writer.container_limit()
        && (writer.container_limit() == 512 || matches!(list.id(), 9 | 10))
    {
        return Err(invalid("NBT nesting exceeds limit"));
    }
    writer.write_all(&[list.id()])?;
    macro_rules! numbers {
        ($v:expr) => {{
            write_length($v.len(), writer)?;
            for x in $v {
                writer.write_all(&x.to_be_bytes())?;
            }
            Ok(())
        }};
    }
    match list {
        NbtList::Empty => write_length(0, writer),
        NbtList::Byte(v) => numbers!(v),
        NbtList::Short(v) => numbers!(v),
        NbtList::Int(v) => numbers!(v),
        NbtList::Long(v) => numbers!(v),
        NbtList::Float(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                writer.write_all(&float_bits(*x).to_be_bytes())?;
            }
            Ok(())
        }
        NbtList::Double(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                writer.write_all(&double_bits(*x).to_be_bytes())?;
            }
            Ok(())
        }
        NbtList::String(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                write_string(x, writer)?;
            }
            Ok(())
        }
        NbtList::Compound(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                write_compound(x, writer, depth + 1)?;
            }
            Ok(())
        }
        NbtList::List(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                write_list(x, writer, depth + 1)?;
            }
            Ok(())
        }
        NbtList::ByteArray(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                write_length(x.len(), writer)?;
                writer.write_all(x)?;
            }
            Ok(())
        }
        NbtList::IntArray(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                write_length(x.len(), writer)?;
                for n in x {
                    writer.write_all(&n.to_be_bytes())?;
                }
            }
            Ok(())
        }
        NbtList::LongArray(v) => {
            write_length(v.len(), writer)?;
            for x in v {
                write_length(x.len(), writer)?;
                for n in x {
                    writer.write_all(&n.to_be_bytes())?;
                }
            }
            Ok(())
        }
    }
}

/// Validates encoded UTF-16 units without allocating a decoded string.
/// Canonical writeUTF output uses one, two, or three bytes per unit, including
/// surrogates. readUTF also accepts overlong forms, but retaining those would
/// admit alternate byte representations through canonical bridge comparison.
fn valid_mutf8(mut bytes: &[u8]) -> bool {
    while let Some((&first, rest)) = bytes.split_first() {
        bytes = rest;
        match first {
            0x01..=0x7f => {}
            0xc0..=0xdf => {
                let Some((&second, rest)) = bytes.split_first() else {
                    return false;
                };
                if second & 0xc0 != 0x80 || (first < 0xc2 && (first != 0xc0 || second != 0x80)) {
                    return false;
                }
                bytes = rest;
            }
            0xe0..=0xef => {
                let [second, third, rest @ ..] = bytes else {
                    return false;
                };
                if second & 0xc0 != 0x80
                    || third & 0xc0 != 0x80
                    || (first == 0xe0 && *second < 0xa0)
                {
                    return false;
                }
                bytes = rest;
            }
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod brewing_rereview_tests {
    use super::*;
    use crate::serial::WriteTo;
    #[test]
    fn brewing_rereview_ordinary_nbt_keeps_storage_order() {
        let mut compound = NbtCompound::new();
        compound.insert("z", 1);
        compound.insert("a", 2);
        let value = NbtTag::Compound(compound);
        let mut expected = Vec::new();
        value.write(&mut expected);
        let mut actual = Vec::new();
        WriteTo::write(&value, &mut actual).expect("ordinary writer");
        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
#[path = "nbt_stream_task14_tests.rs"]
mod task14_tests;
