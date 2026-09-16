use std::io::{Cursor, Error, ErrorKind, Result};

use crate::{codec::VarInt, serial::ReadFrom};

/// Maximum allocation reserved by untrusted network collection decoders.
pub const DEFAULT_COLLECTION_ALLOCATION_BUDGET: usize = 1024 * 1024;

/// Validates that reserving `count` values of `T` fits the caller's allocation budget.
pub fn collection_capacity<T>(count: usize, budget: usize) -> Result<usize> {
    let bytes = count.checked_mul(std::mem::size_of::<T>()).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "collection allocation size overflows",
        )
    })?;
    if bytes > budget {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "collection allocation exceeds budget",
        ));
    }
    Ok(count)
}

/// Reads a collection count after proving that its minimum encoded body fits in
/// the unread cursor bytes.
pub fn read_collection_count(
    data: &mut Cursor<&[u8]>,
    maximum: Option<usize>,
    minimum_bytes_per_element: usize,
) -> Result<usize> {
    let count = usize::try_from(VarInt::read(data)?.0)
        .map_err(|_| Error::new(ErrorKind::InvalidData, "collection count is negative"))?;
    if let Some(maximum) = maximum
        && count > maximum
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "collection count exceeds maximum",
        ));
    }
    let position = usize::try_from(data.position()).map_err(|_| {
        Error::new(
            ErrorKind::InvalidData,
            "cursor position exceeds addressable input",
        )
    })?;
    let remaining = data.get_ref().len().saturating_sub(position);
    let required = count
        .checked_mul(minimum_bytes_per_element)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "collection minimum size overflows"))?;
    if required > remaining {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "collection count exceeds remaining input",
        ));
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, ErrorKind};

    use crate::{
        codec::VarInt,
        serial::{WriteTo as _, budget::read_collection_count},
    };

    #[test]
    fn rejects_a_huge_count_before_a_collection_can_reserve() {
        let mut encoded = Vec::new();
        VarInt(i32::MAX)
            .write(&mut encoded)
            .expect("test count should encode");

        let error = read_collection_count(&mut Cursor::new(encoded.as_slice()), None, 1)
            .expect_err("no element bytes remain");

        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
