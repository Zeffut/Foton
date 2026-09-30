#![expect(
    clippy::disallowed_types,
    reason = "HashMap is used directly to implement WriteTo; project types are not available here"
)]
use std::io;
use std::{
    collections::HashMap,
    hash::BuildHasher,
    io::{Result, Write},
};

use simdnbt::owned::{NbtCompound, NbtTag};
use text_components::TextComponent;
use uuid::Uuid;

use crate::{
    BlockPos, Identifier, PackedBlockPos,
    codec::VarInt,
    serial::{PrefixedWrite, WriteTo},
};

/// Streams owned NBT without allocating a serialization buffer. Validation and
/// exact wire sizing stop as soon as the explicit caller budget is exhausted.
pub fn write_nbt_tag_bounded(tag: &NbtTag, maximum: usize, writer: &mut dyn Write) -> Result<()> {
    super::nbt_stream::write_tag_bounded(tag, maximum, writer)
}

#[cfg(test)]
fn nbt_wire_size(tag: &NbtTag) -> Result<usize> {
    super::nbt_stream::wire_size(tag, usize::MAX)
}

#[cfg(test)]
mod tests {
    use std::io::{ErrorKind, sink};

    use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

    use super::write_nbt_tag_bounded;

    #[test]
    fn bounded_nbt_near_cap_never_allocates_a_growable_temporary() {
        let cap = 1 << 20;
        let mut compound = NbtCompound::new();
        compound.insert("a", NbtTag::ByteArray(vec![0; cap - 15]));
        compound.insert("b", NbtTag::Byte(1));
        let tag = NbtTag::Compound(compound);
        let exact = super::nbt_wire_size(&tag).expect("wire size");
        assert_eq!(exact, cap);
        let mut unbounded = Vec::with_capacity(exact);
        tag.write(&mut unbounded);
        assert!(
            unbounded.capacity() > cap,
            "fixture must expose simdnbt speculative growth"
        );
        let stats = allocation_counter::measure(|| {
            write_nbt_tag_bounded(&tag, cap, &mut sink()).expect("fits exactly");
        });
        assert!(stats.bytes_max <= cap as u64, "{stats:?}");
    }

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
        super::text_stream::write(
            self,
            &mut super::nbt_stream::LimitedWriter::new(writer, usize::MAX),
        )
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
        let length = self
            .namespace
            .len()
            .checked_add(self.path.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| io::Error::other("Identifier length overflow"))?;
        if length > super::DEFAULT_BOUND {
            return Err(io::Error::other("Identifier exceeds network string limit"));
        }
        VarInt(i32::try_from(length).map_err(io::Error::other)?).write(writer)?;
        writer.write_all(self.namespace.as_bytes())?;
        writer.write_all(b":")?;
        writer.write_all(self.path.as_bytes())
    }
}

impl WriteTo for NbtTag {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        super::nbt_stream::write_tag(
            self,
            &mut super::nbt_stream::LimitedWriter::new(writer, usize::MAX),
            0,
        )
    }
}

impl WriteTo for NbtCompound {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        super::nbt_stream::write_compound(
            self,
            &mut super::nbt_stream::LimitedWriter::new(writer, usize::MAX),
            0,
        )
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
                super::nbt_stream::write_compound(
                    compound,
                    &mut super::nbt_stream::LimitedWriter::new(writer, usize::MAX),
                    0,
                )?;
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
