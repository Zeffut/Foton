//! Allocation preflight for NBT-backed component readers.
//!
//! The third-party readers allocate before returning a tag. Inspect the wire first,
//! including declared collection capacities, while the recursive bridge quota is active.
use super::{ReadFrom, budget};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use std::io::{Cursor, Error, Result};

/// Checks an upcoming nameless NBT value without moving the caller's cursor.
/// `typed` includes the largest text/predicate node and conversion workspaces.
pub fn check(data: &Cursor<&[u8]>, typed: bool) -> Result<()> {
    check_copies::<true>(data, typed)
}

/// Preflights one raw owned tree whose recursive readers borrow descendants.
/// Typed leaf conversions separately account for their storage and suffix copies.
pub fn check_single_owner(data: &Cursor<&[u8]>) -> Result<()> {
    check_copies::<false>(data, false)
}

fn check_copies<const SUFFIXES: bool>(data: &Cursor<&[u8]>, typed: bool) -> Result<()> {
    if !budget::is_active() {
        return Ok(());
    }
    let mut cursor = Cursor::new(*data.get_ref());
    cursor.set_position(data.position());
    // Owned readers need no tape. Owned-to-borrowed codec adapters charge their
    // initial tape separately; per-node storage below includes tape growth.
    let id = u8::read(&mut cursor)?;
    payload::<SUFFIXES>(&mut cursor, id, 0, typed)
}
fn skip(data: &mut Cursor<&[u8]>, bytes: usize) -> Result<()> {
    let start = usize::try_from(data.position()).map_err(Error::other)?;
    let end = start
        .checked_add(bytes)
        .filter(|end| *end <= data.get_ref().len())
        .ok_or_else(|| Error::other("truncated NBT"))?;
    data.set_position(end as u64);
    Ok(())
}
fn string<const SUFFIXES: bool>(data: &mut Cursor<&[u8]>, depth: usize) -> Result<()> {
    let length = usize::from(u16::read(data)?);
    // MUTF-8 -> UTF-8 never expands valid input. Include normalization, borrowed
    // tapes/owned copies, and the copies retained by enclosing persistent readers.
    budget::charge::<u8>(length.saturating_mul(6).saturating_mul(if SUFFIXES {
        depth + 1
    } else {
        1
    }))?;
    skip(data, length)
}
fn count(data: &mut Cursor<&[u8]>) -> Result<usize> {
    usize::try_from(i32::read(data)?).map_err(|_| Error::other("negative NBT count"))
}
fn node<const SUFFIXES: bool>(depth: usize, typed: bool) -> Result<()> {
    // Four simultaneous forms: input tree, normalization backing, codec conversion
    // and its result. Enclosing persistent readers can retain a copy at each level.
    let size = if typed {
        size_of::<text_components::TextComponent>().max(size_of::<NbtTag>())
    } else {
        size_of::<NbtTag>()
    };
    budget::charge::<u8>((size + 64).saturating_mul(4).saturating_mul(if SUFFIXES {
        depth + 1
    } else {
        1
    }))
}
fn payload<const SUFFIXES: bool>(
    data: &mut Cursor<&[u8]>,
    id: u8,
    depth: usize,
    typed: bool,
) -> Result<()> {
    if depth > 512 {
        return Err(Error::other("NBT nesting exceeds limit"));
    }
    node::<SUFFIXES>(depth, typed)?;
    match id {
        0 => Ok(()),
        1 => skip(data, 1),
        2 => skip(data, 2),
        3 | 5 => skip(data, 4),
        4 | 6 => skip(data, 8),
        8 => string::<SUFFIXES>(data, depth),
        7 | 11 | 12 => {
            let count = count(data)?;
            let width = match id {
                7 => 1,
                11 => 4,
                _ => 8,
            };
            let bytes = count
                .checked_mul(width)
                .ok_or_else(|| Error::other("NBT array size overflows"))?;
            let storage = if typed {
                count.saturating_mul(size_of::<NbtTag>()).max(bytes)
            } else {
                bytes
            };
            budget::charge::<u8>(storage.saturating_mul(4).saturating_mul(if SUFFIXES {
                depth + 1
            } else {
                1
            }))?;
            skip(data, bytes)
        }
        9 => {
            let element = u8::read(data)?;
            let count = count(data)?;
            if element == 0 && count != 0 {
                return Err(Error::other("nonempty END list"));
            }
            let width = match element {
                0 | 1 => 1,
                2 => 2,
                3 | 5 => 4,
                4 | 6 => 8,
                7 | 11 | 12 => size_of::<Vec<u8>>(),
                8 => size_of::<simdnbt::Mutf8String>(),
                9 => size_of::<NbtList>(),
                10 => size_of::<NbtCompound>(),
                _ => return Err(Error::other("invalid NBT list type")),
            };
            // Charge the declared backing before visiting even the first child.
            budget::charge::<u8>(
                count
                    .saturating_mul(width)
                    .saturating_mul(4)
                    .saturating_mul(if SUFFIXES { depth + 1 } else { 1 }),
            )?;
            for _ in 0..count {
                payload::<SUFFIXES>(data, element, depth + 1, typed)?;
            }
            Ok(())
        }
        10 => {
            loop {
                let child = u8::read(data)?;
                if child == 0 {
                    break;
                }
                string::<SUFFIXES>(data, depth + 1)?;
                payload::<SUFFIXES>(data, child, depth + 1, typed)?;
            }
            Ok(())
        }
        _ => Err(Error::other("invalid NBT tag type")),
    }
}

/// Accounts an owned leaf's subsequent typed conversion under the active quota.
/// Serialization is bounded before allocating its temporary wire buffer.
pub fn check_owned(tag: &NbtTag, maximum: usize) -> Result<()> {
    if !budget::is_active() {
        return Ok(());
    }
    let length = super::nbt_stream::wire_size(tag, maximum)?;
    let mut bytes = budget::read_vec(length, length)?;
    super::nbt_stream::write_tag(
        tag,
        &mut super::nbt_stream::LimitedWriter::new(&mut bytes, maximum),
        0,
    )?;
    check(&Cursor::new(bytes.as_slice()), true)
}
