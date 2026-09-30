//! Borrowed persistent-codec building blocks for budgeted output.
use super::{
    nbt_stream::{self, LimitedWriter},
    text_stream,
};
use crate::serial::nbt_stream::NbtWrite;
use simdnbt::owned::{NbtCompound, NbtTag};
use std::io::{Error, ErrorKind, Result, Write};

/// A persistent NBT value that can be encoded without creating an owned tree.
pub trait NbtEncode {
    /// The concrete NBT tag ID of this codec value.
    fn nbt_id(&self) -> u8;
    /// Writes the payload, respecting the supplied recursive nesting depth.
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()>;
}
/// Streams a complete nameless tag with a caller-supplied byte budget.
pub fn write_bounded(
    value: &(impl NbtEncode + ?Sized),
    budget: usize,
    writer: &mut dyn Write,
) -> Result<()> {
    let mut writer = LimitedWriter::persistent(writer, budget);
    writer.write_all(&[value.nbt_id()])?;
    value.write_nbt_payload(&mut writer, 0)
}
/// Writes a named compound field directly from its original value.
pub fn field(
    name: &str,
    value: &(impl NbtEncode + ?Sized),
    writer: &mut dyn NbtWrite,
    depth: usize,
) -> Result<()> {
    check_depth(depth)?;
    writer.write_all(&[value.nbt_id()])?;
    text_stream::write_utf(name, writer)?;
    if value.nbt_id() == 10 {
        check_depth(depth + 1)?;
    }
    value.write_nbt_payload(writer, depth + 1)
}
/// Checks a compound depth against the registered persistent reader's stack ceiling.
pub fn check_depth(depth: usize) -> Result<()> {
    if depth >= 511 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "NBT nesting exceeds limit",
        ));
    }
    Ok(())
}
/// Finishes a compound payload.
pub fn end(writer: &mut dyn NbtWrite) -> Result<()> {
    writer.write_all(&[0])
}
/// Writes a list payload, preserving the codec's heterogeneous-compound wrapping.
pub fn list<T: NbtEncode>(values: &[T], writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    writer.ensure_remaining(values.len().saturating_add(5))?;
    let first = values.first().map_or(10, NbtEncode::nbt_id);
    let homogeneous = values.iter().all(|value| value.nbt_id() == first);
    let id = if homogeneous { first } else { 10 };
    if matches!(id, 9 | 10) {
        check_depth(depth)?;
    }
    writer.write_all(&[id])?;
    writer.write_all(
        &i32::try_from(values.len())
            .map_err(Error::other)?
            .to_be_bytes(),
    )?;
    for value in values {
        if !homogeneous && value.nbt_id() != 10 {
            field("", value, writer, depth + 1)?;
            end(writer)?;
        } else {
            if value.nbt_id() == 10 {
                check_depth(depth + 1)?;
            }
            value.write_nbt_payload(writer, depth + 1)?;
        }
    }
    Ok(())
}
impl<T: NbtEncode> NbtEncode for Vec<T> {
    fn nbt_id(&self) -> u8 {
        9
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        list(self, writer, depth)
    }
}
impl NbtEncode for NbtTag {
    fn nbt_id(&self) -> u8 {
        self.id()
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        nbt_stream::write_payload(self, writer, depth)
    }
}
impl NbtEncode for NbtCompound {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        nbt_stream::write_compound(self, writer, depth)
    }
}
impl NbtEncode for str {
    fn nbt_id(&self) -> u8 {
        8
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, _depth: usize) -> Result<()> {
        text_stream::write_utf(self, writer)
    }
}
impl NbtEncode for String {
    fn nbt_id(&self) -> u8 {
        8
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, _depth: usize) -> Result<()> {
        text_stream::write_utf(self, writer)
    }
}
impl NbtEncode for text_components::TextComponent {
    fn nbt_id(&self) -> u8 {
        text_stream::tag_id(self)
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        text_stream::write_payload(self, writer, depth)
    }
}
macro_rules! number {
    ($type:ty, $id:literal) => {
        impl NbtEncode for $type {
            fn nbt_id(&self) -> u8 {
                $id
            }
            fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, _depth: usize) -> Result<()> {
                writer.write_all(&self.to_be_bytes())
            }
        }
    };
}
number!(i8, 1);
number!(i16, 2);
number!(i32, 3);
number!(i64, 4);
impl NbtEncode for f32 {
    fn nbt_id(&self) -> u8 {
        5
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, _depth: usize) -> Result<()> {
        writer.write_all(&nbt_stream::float_bits(*self).to_be_bytes())
    }
}
impl NbtEncode for f64 {
    fn nbt_id(&self) -> u8 {
        6
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, _depth: usize) -> Result<()> {
        writer.write_all(&nbt_stream::double_bits(*self).to_be_bytes())
    }
}
impl NbtEncode for bool {
    fn nbt_id(&self) -> u8 {
        1
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, _depth: usize) -> Result<()> {
        writer.write_all(&[u8::from(*self)])
    }
}

impl NbtEncode for crate::Identifier {
    fn nbt_id(&self) -> u8 {
        8
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, _depth: usize) -> Result<()> {
        let length = self
            .namespace
            .len()
            .checked_add(self.path.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| Error::other("identifier size overflow"))?;
        let length = u16::try_from(length).map_err(Error::other)?;
        writer.write_all(&length.to_be_bytes())?;
        writer.write_all(self.namespace.as_bytes())?;
        writer.write_all(b":")?;
        writer.write_all(self.path.as_bytes())
    }
}

/// Writes a named field with an explicit tag ID and a borrowed payload encoder.
pub fn field_with(
    name: &str,
    id: u8,
    writer: &mut dyn NbtWrite,
    encode: impl FnOnce(&mut dyn NbtWrite) -> Result<()>,
) -> Result<()> {
    writer.write_all(&[id])?;
    text_stream::write_utf(name, writer)?;
    encode(writer)
}
impl<T: NbtEncode + ?Sized> NbtEncode for &T {
    fn nbt_id(&self) -> u8 {
        (**self).nbt_id()
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        (**self).write_nbt_payload(writer, depth)
    }
}
impl NbtEncode for () {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        check_depth(depth)?;
        end(writer)
    }
}

impl<T: NbtEncode + ?Sized> NbtEncode for Box<T> {
    fn nbt_id(&self) -> u8 {
        (**self).nbt_id()
    }
    fn write_nbt_payload(&self, w: &mut dyn NbtWrite, d: usize) -> Result<()> {
        (**self).write_nbt_payload(w, d)
    }
}

/// Writes a registry-keyed field without formatting an owned identifier.
pub fn identifier_field(
    name: &crate::Identifier,
    value: &(impl NbtEncode + ?Sized),
    writer: &mut dyn NbtWrite,
    depth: usize,
) -> Result<()> {
    check_depth(depth)?;
    writer.write_all(&[value.nbt_id()])?;
    name.write_nbt_payload(writer, depth)?;
    if value.nbt_id() == 10 {
        check_depth(depth + 1)?;
    }
    value.write_nbt_payload(writer, depth + 1)
}

impl NbtEncode for uuid::Uuid {
    fn nbt_id(&self) -> u8 {
        11
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, _depth: usize) -> Result<()> {
        writer.write_all(&4_i32.to_be_bytes())?;
        writer.write_all(self.as_bytes())
    }
}

#[cfg(test)]
mod task14_typed_float_tests {
    use super::write_bounded;

    const F32_CASES: [(u32, u32); 8] = [
        (0x7f80_0001, 0x7fc0_0000),
        (0xffc1_2345, 0x7fc0_0000),
        (0x0000_0000, 0x0000_0000),
        (0x8000_0000, 0x8000_0000),
        (0x7f80_0000, 0x7f80_0000),
        (0xff80_0000, 0xff80_0000),
        (0x3f80_0000, 0x3f80_0000),
        (0x0000_0001, 0x0000_0001),
    ];
    const F64_CASES: [(u64, u64); 8] = [
        (0x7ff0_0000_0000_0001, 0x7ff8_0000_0000_0000),
        (0xfff8_1234_5678_9abc, 0x7ff8_0000_0000_0000),
        (0x0000_0000_0000_0000, 0x0000_0000_0000_0000),
        (0x8000_0000_0000_0000, 0x8000_0000_0000_0000),
        (0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_0000),
        (0xfff0_0000_0000_0000, 0xfff0_0000_0000_0000),
        (0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0000),
        (0x0000_0000_0000_0001, 0x0000_0000_0000_0001),
    ];

    #[test]
    fn task14_typed_f32_scalar_and_list_use_java_nan_bits_and_exact_caps() {
        let mut list_expected = vec![9, 5, 0, 0, 0, F32_CASES.len() as u8];
        let values: Vec<_> = F32_CASES
            .iter()
            .map(|(input, expected)| {
                let value = f32::from_bits(*input);
                let mut actual = Vec::new();
                write_bounded(&value, 5, &mut actual).expect("exact scalar cap");
                let mut scalar_expected = vec![5];
                scalar_expected.extend(expected.to_be_bytes());
                assert_eq!(actual, scalar_expected, "input bits {input:#010x}");
                assert!(write_bounded(&value, 4, &mut Vec::new()).is_err());
                list_expected.extend(expected.to_be_bytes());
                value
            })
            .collect();

        let mut actual = Vec::new();
        write_bounded(&values, list_expected.len(), &mut actual).expect("exact list cap");
        assert_eq!(actual, list_expected);
        assert!(write_bounded(&values, actual.len() - 1, &mut Vec::new()).is_err());
    }

    #[test]
    fn task14_typed_f64_scalar_and_list_use_java_nan_bits_and_exact_caps() {
        let mut list_expected = vec![9, 6, 0, 0, 0, F64_CASES.len() as u8];
        let values: Vec<_> = F64_CASES
            .iter()
            .map(|(input, expected)| {
                let value = f64::from_bits(*input);
                let mut actual = Vec::new();
                write_bounded(&value, 9, &mut actual).expect("exact scalar cap");
                let mut scalar_expected = vec![6];
                scalar_expected.extend(expected.to_be_bytes());
                assert_eq!(actual, scalar_expected, "input bits {input:#018x}");
                assert!(write_bounded(&value, 8, &mut Vec::new()).is_err());
                list_expected.extend(expected.to_be_bytes());
                value
            })
            .collect();

        let mut actual = Vec::new();
        write_bounded(&values, list_expected.len(), &mut actual).expect("exact list cap");
        assert_eq!(actual, list_expected);
        assert!(write_bounded(&values, actual.len() - 1, &mut Vec::new()).is_err());
    }
}

#[cfg(test)]
mod brewing_rereview_tests {
    use super::*;
    use std::{cell::Cell, io::sink};

    struct Counted<'a>(&'a Cell<usize>);
    impl NbtEncode for Counted<'_> {
        fn nbt_id(&self) -> u8 {
            self.0.set(self.0.get() + 1);
            8
        }
        fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, _: usize) -> Result<()> {
            writer.write_all(&[0, 0])
        }
    }
    #[test]
    fn brewing_rereview_typed_list_rejects_minimum_before_classification() {
        let work = Cell::new(0);
        let values: Vec<_> = (0..100_000).map(|_| Counted(&work)).collect();
        assert!(write_bounded(&values, 16, &mut sink()).is_err());
        eprintln!("typed list classification visits: {}", work.get());
        assert!(work.get() <= 16, "scanned impossible list: {}", work.get());
    }
}

#[cfg(test)]
mod brewing_rereview_depth_tests {
    use super::*;
    use simdnbt::{borrow::read_tag, owned::NbtList};
    use std::{
        io::{Cursor, sink},
        thread::Builder,
    };
    enum Container {
        End,
        Compound(Box<Self>),
        List(Vec<Self>),
    }
    impl NbtEncode for Container {
        fn nbt_id(&self) -> u8 {
            if matches!(self, Self::List(_)) { 9 } else { 10 }
        }
        fn write_nbt_payload(&self, w: &mut dyn NbtWrite, d: usize) -> Result<()> {
            match self {
                Self::List(v) => list(v, w, d),
                Self::Compound(v) => {
                    check_depth(d)?;
                    field("x", v, w, d)?;
                    end(w)
                }
                Self::End => {
                    check_depth(d)?;
                    end(w)
                }
            }
        }
    }
    #[test]
    fn brewing_rereview_typed_containers_match_registered_reader_depth() {
        Builder::new()
            .stack_size(32 << 20)
            .spawn(|| {
                for shape in 0..3 {
                    for count in [511, 512, 513] {
                        let mut value = Container::End;
                        for level in 1..count {
                            value = if shape == 0 || (shape == 2 && level % 2 == 0) {
                                Container::Compound(Box::new(value))
                            } else {
                                Container::List(vec![value])
                            };
                        }
                        let result = write_bounded(&value, 256 * 1024, &mut sink());
                        let mut tag = NbtTag::Compound(NbtCompound::new());
                        for level in 1..count {
                            tag = if shape == 0 || (shape == 2 && level % 2 == 0) {
                                let mut c = NbtCompound::new();
                                c.insert("x", tag);
                                NbtTag::Compound(c)
                            } else {
                                NbtTag::List(NbtList::from(vec![tag]))
                            };
                        }
                        let mut bytes = Vec::new();
                        tag.write(&mut bytes);
                        let accepted = read_tag(&mut Cursor::new(bytes.as_slice())).is_ok();
                        assert_eq!(
                            result.is_ok(),
                            accepted,
                            "typed shape={shape} containers={count}"
                        );
                    }
                }
            })
            .expect("worker")
            .join()
            .expect("depth tests");
    }
}
