use std::{
    cell::Cell,
    io::{Cursor, Error, ErrorKind, Result},
    mem::size_of,
};

use crate::{codec::VarInt, serial::ReadFrom};

/// Maximum allocation reserved by untrusted network collection decoders.
pub const DEFAULT_COLLECTION_ALLOCATION_BUDGET: usize = 1024 * 1024;

/// One allocation allowance shared by all collections in a recursive decode.
/// Reservations are not refunded: successfully decoded children remain owned
/// by their parents until the complete decode finishes.
pub struct DecodeBudget {
    remaining: usize,
}

impl DecodeBudget {
    /// Starts a decode with an explicit aggregate reservation limit.
    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self { remaining: bytes }
    }
}

thread_local! {
    static ACTIVE: Cell<Option<usize>> = const { Cell::new(None) };
}

impl DecodeBudget {
    /// Runs synchronous component readers against this same recursive allowance.
    /// The guard restores the caller's scope even when decoding unwinds.
    pub fn decode<T>(&mut self, read: impl FnOnce() -> Result<T>) -> Result<T> {
        struct Scope<'a>(&'a mut DecodeBudget);
        impl Drop for Scope<'_> {
            fn drop(&mut self) {
                if let Some(remaining) = ACTIVE.with(|active| active.replace(None)) {
                    self.0.remaining = remaining;
                }
            }
        }
        if is_active() {
            return read();
        }
        ACTIVE.with(|active| active.set(Some(self.remaining)));
        let _scope = Scope(self);
        read()
    }
}

/// Whether a synchronous bounded component decode is in progress.
#[must_use]
pub fn is_active() -> bool {
    ACTIVE.with(|active| active.get().is_some())
}

/// Charges retained or temporary storage before a component reader allocates it.
pub fn charge<T>(count: usize) -> Result<()> {
    ACTIVE.with(|active| {
        if let Some(remaining) = active.get() {
            collection_capacity::<T>(count, remaining)?;
            active.set(Some(remaining - count * size_of::<T>()));
        }
        Ok(())
    })
}

/// Reserves a complete declared vector only after charging it to the shared quota.
/// Ordinary readers keep their existing initial-capacity policy.
pub fn read_vec<T>(count: usize, ordinary_capacity: usize) -> Result<Vec<T>> {
    if !is_active() {
        return Ok(Vec::with_capacity(ordinary_capacity));
    }
    charge::<T>(count)?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(Error::other)?;
    Ok(values)
}

/// Checks a concrete codec's minimum body before reserving declared storage.
/// Generic codecs that may consume zero bytes must not assume a nonzero minimum.
pub fn check_collection_input(
    data: &Cursor<&[u8]>,
    count: usize,
    minimum_bytes_per_element: usize,
) -> Result<()> {
    let remaining = data
        .get_ref()
        .len()
        .saturating_sub(usize::try_from(data.position()).map_err(Error::other)?);
    let required = count
        .checked_mul(minimum_bytes_per_element)
        .ok_or_else(|| Error::other("collection minimum size overflows"))?;
    if required > remaining {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "collection exceeds remaining input",
        ));
    }
    Ok(())
}

/// Pays for hash-table buckets, control bytes, and geometric reallocation overlap.
/// Four slots is hashbrown's smallest table; subsequent tables stay below a 7/8 load.
pub fn charge_map<K, V>(count: usize) -> Result<()> {
    if count == 0 || !is_active() {
        return Ok(());
    }
    let buckets = count
        .checked_mul(8)
        .and_then(|n| n.checked_div(7))
        .and_then(|n| n.checked_add(1))
        .and_then(usize::checked_next_power_of_two)
        .ok_or_else(|| Error::other("map allocation overflows"))?
        .max(4);
    let bytes = buckets
        .checked_mul(size_of::<(K, V)>() + 1)
        .and_then(|n| n.checked_add(16))
        .and_then(|n| n.checked_mul(2))
        .ok_or_else(|| Error::other("map allocation overflows"))?;
    charge::<u8>(bytes)
}

/// Validates that reserving `count` values of `T` fits the caller's allocation budget.
pub fn collection_capacity<T>(count: usize, budget: usize) -> Result<usize> {
    let bytes = count.checked_mul(size_of::<T>()).ok_or_else(|| {
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
    check_collection_input(data, count, minimum_bytes_per_element)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, ErrorKind},
        mem::size_of,
    };

    use crate::{
        codec::VarInt,
        serial::{
            WriteTo as _,
            budget::{DecodeBudget, read_collection_count},
        },
    };

    #[test]
    fn overflowing_charge_preserves_budget_for_an_exact_followup() {
        let mut budget = DecodeBudget::new(size_of::<u64>());

        budget
            .decode(|| {
                assert!(super::charge::<u64>(usize::MAX).is_err());
                super::charge::<u64>(1)?;
                assert!(super::charge::<u64>(1).is_err());
                Ok(())
            })
            .expect("exact charge fits");
    }

    #[test]
    fn nested_scopes_cannot_reset_the_budget_and_errors_restore_it() {
        let mut outer = DecodeBudget::new(8);
        let result = outer.decode(|| {
            super::charge::<u8>(5)?;
            DecodeBudget::new(1024).decode(|| super::charge::<u8>(4))
        });
        assert!(result.is_err());
        assert!(!super::is_active());
        outer
            .decode(|| super::charge::<u8>(3))
            .expect("failed charge did not consume quota");
        assert!(outer.decode(|| super::charge::<u8>(1)).is_err());
        DecodeBudget::new(8)
            .decode(|| super::charge::<u8>(8))
            .expect("independent scope");
    }

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
