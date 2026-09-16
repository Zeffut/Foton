#![expect(
    clippy::disallowed_types,
    reason = "HashMap is used directly to implement WriteTo; project types are not available here"
)]
use std::{
    collections::HashMap,
    hash::BuildHasher,
    io::{Result, Write},
};

use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use text_components::TextComponent;
use uuid::Uuid;

use crate::{
    BlockPos, Identifier, PackedBlockPos,
    codec::VarInt,
    serial::{PrefixedWrite, WriteTo},
};

/// Writes an NBT tag after rejecting values that would exceed the temporary
/// serialization budget.
pub fn write_nbt_tag_bounded(tag: &NbtTag, maximum: usize, writer: &mut dyn Write) -> Result<()> {
    let wire_size = nbt_wire_size(tag)?;
    if wire_size > maximum {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "NBT value exceeds serialization budget",
        ));
    }
    let mut buf = Vec::new();
    buf.try_reserve_exact(wire_size)
        .map_err(|_| std::io::Error::other("NBT serialization allocation failed"))?;
    tag.write(&mut buf);
    writer.write_all(&buf)
}

fn nbt_wire_size(tag: &NbtTag) -> Result<usize> {
    nbt_payload_size(tag)?
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("NBT wire size overflows"))
}

fn nbt_payload_size(tag: &NbtTag) -> Result<usize> {
    match tag {
        NbtTag::Byte(_) => Ok(1),
        NbtTag::Short(_) => Ok(2),
        NbtTag::Int(_) | NbtTag::Float(_) => Ok(4),
        NbtTag::Long(_) | NbtTag::Double(_) => Ok(8),
        NbtTag::ByteArray(values) => sized(4, values.len(), 1),
        NbtTag::String(value) => value
            .len()
            .checked_add(2)
            .ok_or_else(|| std::io::Error::other("NBT wire size overflows")),
        NbtTag::List(list) => nbt_list_wire_size(list),
        NbtTag::Compound(compound) => nbt_compound_wire_size(compound),
        NbtTag::IntArray(values) => sized(4, values.len(), 4),
        NbtTag::LongArray(values) => sized(4, values.len(), 8),
    }
}

fn nbt_compound_wire_size(compound: &NbtCompound) -> Result<usize> {
    compound.iter().try_fold(1usize, |size, (name, value)| {
        let value_size = nbt_payload_size(value)?;
        size.checked_add(3)
            .and_then(|size| size.checked_add(name.len()))
            .and_then(|size| size.checked_add(value_size))
            .ok_or_else(|| std::io::Error::other("NBT wire size overflows"))
    })
}

fn nbt_list_wire_size(list: &NbtList) -> Result<usize> {
    let header = 5usize;
    match list {
        NbtList::Empty => Ok(header),
        NbtList::Byte(v) => sized(header, v.len(), 1),
        NbtList::Short(v) => sized(header, v.len(), 2),
        NbtList::Int(v) => sized(header, v.len(), 4),
        NbtList::Float(v) => sized(header, v.len(), 4),
        NbtList::Long(v) => sized(header, v.len(), 8),
        NbtList::Double(v) => sized(header, v.len(), 8),
        NbtList::String(v) => v
            .iter()
            .try_fold(header, |size, value| add(size, value.len().checked_add(2))),
        NbtList::Compound(v) => v.iter().try_fold(header, |size, value| {
            add(size, Some(nbt_compound_wire_size(value)?))
        }),
        NbtList::List(v) => v.iter().try_fold(header, |size, value| {
            add(size, Some(nbt_list_wire_size(value)?))
        }),
        NbtList::ByteArray(v) => v.iter().try_fold(header, |size, value| {
            add(size, Some(sized(4, value.len(), 1)?))
        }),
        NbtList::IntArray(v) => v.iter().try_fold(header, |size, value| {
            add(size, Some(sized(4, value.len(), 4)?))
        }),
        NbtList::LongArray(v) => v.iter().try_fold(header, |size, value| {
            add(size, Some(sized(4, value.len(), 8)?))
        }),
    }
}

fn sized(header: usize, count: usize, width: usize) -> Result<usize> {
    add(header, count.checked_mul(width))
}
fn add(size: usize, extra: Option<usize>) -> Result<usize> {
    size.checked_add(extra.ok_or_else(|| std::io::Error::other("NBT wire size overflows"))?)
        .ok_or_else(|| std::io::Error::other("NBT wire size overflows"))
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use simdnbt::owned::{NbtList, NbtTag};

    use super::write_nbt_tag_bounded;

    #[test]
    fn bounded_nbt_uses_mutf8_wire_length_not_java_heap_accounting() {
        let tag = NbtTag::List(NbtList::String(
            (0..18).map(|_| "\u{0800}".repeat(20_000).into()).collect(),
        ));
        let mut output = Vec::new();

        let error = write_nbt_tag_bounded(&tag, 1024 * 1024, &mut output)
            .expect_err("three-byte BMP MUTF-8 strings exceed the one-MiB wire budget");

        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(output.is_empty());
    }
}

impl WriteTo for bool {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        u8::from(*self).write(writer)?;
        Ok(())
    }
}

impl WriteTo for u8 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl WriteTo for u16 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl WriteTo for u32 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl WriteTo for u64 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl WriteTo for i8 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl WriteTo for i16 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl WriteTo for i32 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl WriteTo for i64 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl WriteTo for f32 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl WriteTo for f64 {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.to_be_bytes())
    }
}

impl<T: WriteTo> WriteTo for Option<T> {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        if let Some(value) = self {
            true.write(writer)?;
            value.write(writer)
        } else {
            false.write(writer)
        }
    }
}

impl<T: WriteTo, const N: usize> WriteTo for [T; N] {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        for i in self {
            i.write(writer)?;
        }
        Ok(())
    }
}

impl<T: WriteTo, Z: WriteTo> WriteTo for (T, Z) {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.0.write(writer)?;
        self.1.write(writer)
    }
}

impl<K: WriteTo, V: WriteTo, S: BuildHasher> WriteTo for HashMap<K, V, S> {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.len() as i32).write(writer)?;
        for (key, value) in self {
            key.write(writer)?;
            value.write(writer)?;
        }
        Ok(())
    }
}

impl<T: WriteTo> WriteTo for Vec<T> {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.write_prefixed::<VarInt>(writer)
    }
}

impl WriteTo for BlockPos {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        PackedBlockPos::from(*self).write(writer)
    }
}

impl WriteTo for TextComponent {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        WriteTo::write(&self.to_codec_nbt(), writer)
    }
}

impl WriteTo for Uuid {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        let (most_significant_bits, least_significant_bits) = self.as_u64_pair();
        most_significant_bits.write(writer)?;
        least_significant_bits.write(writer)?;
        Ok(())
    }
}

impl WriteTo for Identifier {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.to_string().write_prefixed::<VarInt>(writer)?;
        Ok(())
    }
}

impl WriteTo for NbtTag {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        let mut buf = Vec::new();
        self.write(&mut buf);
        writer.write_all(&buf)
    }
}

impl WriteTo for NbtCompound {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        let mut buf = Vec::new();
        self.write(&mut buf);
        writer.write_all(&buf)?;
        Ok(())
    }
}

/// Wrapper for optional NBT that uses the protocol format (END tag for None).
///
/// This is different from `Option<NbtCompound>` which writes a boolean prefix.
/// In the Minecraft protocol, nullable NBT is represented as:
/// - Present: the compound tag bytes
/// - Absent: a single END tag byte (0x00)
#[derive(Debug, Clone)]
pub struct OptionalNbt(pub Option<NbtCompound>);

impl WriteTo for OptionalNbt {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        match &self.0 {
            Some(compound) => {
                // Write compound tag type (0x0A) first, then the compound contents
                // This matches vanilla's writeAnyTag format
                writer.write_all(&[0x0A])?;
                let mut buf = Vec::new();
                compound.write(&mut buf);
                writer.write_all(&buf)?;
            }
            None => {
                // Write END tag (0x00) for null/absent NBT
                writer.write_all(&[0x00])?;
            }
        }
        Ok(())
    }
}

impl From<Option<NbtCompound>> for OptionalNbt {
    fn from(opt: Option<NbtCompound>) -> Self {
        Self(opt)
    }
}
