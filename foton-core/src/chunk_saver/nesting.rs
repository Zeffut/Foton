//! A depth ceiling for the self-nesting shapes in the persistence format.
//!
//! Two persisted types contain themselves: [`PersistentEntity`] holds its
//! passengers, and `PersistentPoolElement::List` holds sub-elements. Both are
//! read by a derived `SchemaRead`, which recurses once per nesting level, and
//! `wincode` has no depth limit of its own. A corrupt or crafted region file
//! nests tens of thousands of levels in a few hundred compressed bytes and
//! overflows the stack.
//!
//! That is worse than an ordinary panic. A stack overflow is a SIGSEGV, so
//! `clear_corrupt_chunk_if_unchanged` never runs, the slot is never
//! quarantined, and the server dies again on the next start as soon as
//! something asks for that chunk. The byte quota does not help: it bounds
//! bytes, and a measured thirteen of them buy a level.
//!
//! Vanilla solves the same problem one layer down, in `NbtAccounter`: every
//! compound and list calls `pushDepth`, which throws once past
//! `MAX_STACK_DEPTH`. The exception is recoverable, so the chunk is rejected
//! rather than the process killed. [`Nested`] is that mechanism, applied to the
//! two edges that can actually recurse -- but with a ceiling of its own, for
//! the reason [`MAX_NESTING_DEPTH`] gives.
//!
//! [`PersistentEntity`]: super::format::PersistentEntity

use core::cell::Cell;
use core::mem::MaybeUninit;

use wincode::config::ConfigCore;
use wincode::io::{Reader, Writer};
use wincode::{ReadError, ReadResult, SchemaRead, SchemaWrite, WriteResult};

/// Deepest nesting a persisted value may reach.
///
/// Vanilla's own ceiling is 512 (`NbtAccounter.MAX_STACK_DEPTH`), and that
/// number does not transfer: vanilla's NBT frames are a hundred bytes or so,
/// while a derived `SchemaRead` frame here is measured in kilobytes. On the
/// 2 MiB stack tokio gives a worker, a debug build decodes 96 levels and
/// overflows at 128, so 512 would be a stack overflow dressed as a limit.
///
/// 32 is chosen from the data instead of from vanilla's constant. The deepest
/// `list_pool_element` nesting in the whole of vanilla's built-in datapacks is
/// **1**, and a passenger chain past a handful is already something only
/// `/ride` can build. 32 is thirty-two times the deepest real value and still
/// a third of the measured debug ceiling, which leaves the frames already on
/// the stack when the recursion starts their own room.
///
/// The trade-off is named rather than hidden, and it is bigger than it looks:
/// a refusal is a decode failure, and a decode failure clears the slot so
/// worldgen refills the column -- so a world that really did stack more than 32
/// riders loses that column, not just its riders. That is why
/// `quarantine_chunk_bytes` exists: the bytes are copied aside first, so the
/// cost is a column to restore by hand rather than one destroyed. The price is
/// still worth paying against a SIGSEGV that no handler can catch.
pub(super) const MAX_NESTING_DEPTH: u32 = 32;

thread_local! {
    /// Levels of [`Nested`] currently open on this thread.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Holds one level of nesting open, and gives it back on drop.
struct DepthGuard;

impl DepthGuard {
    /// Opens a level, or refuses if the ceiling is already reached.
    fn enter() -> ReadResult<Self> {
        DEPTH.with(|depth| {
            let current = depth.get();
            if current >= MAX_NESTING_DEPTH {
                return Err(ReadError::Custom(
                    "persisted data nests deeper than the format allows",
                ));
            }
            depth.set(current + 1);
            Ok(Self)
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// A value on a recursive edge of the persistence format.
///
/// Serializes exactly as the value it wraps -- the wrapper is transparent on
/// the wire, so no saved world changes meaning -- but reading one costs a level
/// of the depth budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nested<T>(pub T);

impl<T> Nested<T> {
    /// Unwraps to the value inside.
    pub fn into_inner(self) -> T {
        self.0
    }
}

// SAFETY: `read` initializes `dst` only on the `Ok` path, and only with what
// the inner schema's own `get` returned, which is initialized because `get`
// returned `Ok`. The refusal path returns before `dst` is touched. `TYPE_META`
// is left at the default `Dynamic`, which the trait documents as always safe.
unsafe impl<'de, C, T> SchemaRead<'de, C> for Nested<T>
where
    C: ConfigCore,
    T: SchemaRead<'de, C>,
{
    type Dst = Nested<T::Dst>;

    fn read(reader: impl Reader<'de>, dst: &mut MaybeUninit<Self::Dst>) -> ReadResult<()> {
        let _guard = DepthGuard::enter()?;
        let inner = <T as SchemaRead<'de, C>>::get(reader)?;
        dst.write(Nested(inner));
        Ok(())
    }
}

// SAFETY: both methods forward to the inner schema without writing a byte of
// their own, so `size_of` reports exactly what `write` writes whenever the
// inner schema does. `TYPE_META` is left at the default `Dynamic`.
unsafe impl<C, T> SchemaWrite<C> for Nested<T>
where
    C: ConfigCore,
    T: SchemaWrite<C>,
    T::Src: Sized,
{
    type Src = Nested<T::Src>;

    fn size_of(src: &Self::Src) -> WriteResult<usize> {
        <T as SchemaWrite<C>>::size_of(&src.0)
    }

    fn write(writer: impl Writer, src: &Self::Src) -> WriteResult<()> {
        <T as SchemaWrite<C>>::write(writer, &src.0)
    }
}

#[cfg(test)]
mod tests {
    use std::thread::Builder;

    use foton_utils::Identifier;

    use super::*;
    use crate::chunk_saver::{PersistentEntity, PersistentPoolElement};

    /// Tokio's default worker and blocking thread stack. The decode runs on one
    /// of those, so the ceiling has to fit inside it.
    const PRODUCTION_STACK_BYTES: usize = 2 * 1024 * 1024;

    /// A pool element tree that is `levels` deep below its root.
    fn pool_element_nested(levels: u32) -> PersistentPoolElement {
        let mut element = PersistentPoolElement::Empty;
        for _ in 0..levels {
            element = PersistentPoolElement::List {
                elements: vec![Nested(element)],
                projection: 0,
            };
        }
        element
    }

    /// A passenger chain that is `levels` deep below its driver.
    fn entity_nested(levels: u32) -> PersistentEntity {
        let leaf = || PersistentEntity {
            entity_type: Identifier::vanilla_static("pig"),
            uuid: [7; 16],
            pos: [0.0; 3],
            motion: [0.0; 3],
            rotation: [0.0; 2],
            fall_distance: 0.0,
            remaining_fire_ticks: 0,
            ticks_frozen: 0,
            is_in_powder_snow: false,
            was_in_powder_snow: false,
            has_visual_fire: false,
            on_ground: false,
            no_gravity: false,
            invulnerable: false,
            air_supply: 0,
            portal_cooldown: 0,
            custom_name_nbt: Vec::new(),
            custom_name_visible: false,
            silent: false,
            glowing: false,
            tags: Vec::new(),
            custom_data_nbt: Vec::new(),
            nbt_data: Vec::new(),
            passengers: Vec::new(),
        };

        let mut entity = leaf();
        for _ in 0..levels {
            let mut vehicle = leaf();
            vehicle.passengers = vec![Nested(entity)];
            entity = vehicle;
        }
        entity
    }

    /// Runs `body` on a thread with exactly the stack production gives it.
    fn on_a_production_sized_stack<T: Send + 'static>(
        body: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        Builder::new()
            .stack_size(PRODUCTION_STACK_BYTES)
            .spawn(body)
            .expect("test thread should spawn")
            .join()
            .expect("decode should not kill its thread")
    }

    #[test]
    fn nesting_past_the_ceiling_is_refused_instead_of_overflowing_the_stack() {
        let bomb = pool_element_nested(MAX_NESTING_DEPTH + 1);
        let encoded = wincode::serialize(&bomb).expect("the bomb should encode");

        let decoded = on_a_production_sized_stack(move || {
            wincode::deserialize_exact::<PersistentPoolElement>(&encoded).is_err()
        });

        assert!(
            decoded,
            "a tree one level past the ceiling must be refused, not decoded"
        );
    }

    #[test]
    fn the_ceiling_itself_still_decodes_on_a_production_sized_stack() {
        // The outermost value is read directly, so `MAX_NESTING_DEPTH` wrapped
        // levels sit exactly on the ceiling and must still be accepted. This is
        // the test that earns the constant: vanilla's 512 is only usable here if
        // 512 of *our* frames fit in the stack production actually gives them.
        let deepest_allowed = entity_nested(MAX_NESTING_DEPTH);
        let encoded = wincode::serialize(&deepest_allowed).expect("the chain should encode");

        let depth = on_a_production_sized_stack(move || {
            let mut entity = wincode::deserialize_exact::<PersistentEntity>(&encoded)
                .expect("a chain on the ceiling should decode");
            let mut depth = 0_u32;
            while let Some(passenger) = entity.passengers.pop() {
                entity = passenger.0;
                depth += 1;
            }
            depth
        });

        assert_eq!(depth, MAX_NESTING_DEPTH);
    }

    #[test]
    fn a_refused_read_gives_its_depth_budget_back() {
        // The counter is thread-local and shared by both nesting shapes, so a
        // refusal that leaked levels would poison every later read on the same
        // worker thread -- one bad chunk would reject good ones.
        on_a_production_sized_stack(|| {
            let bomb = pool_element_nested(MAX_NESTING_DEPTH + 1);
            let encoded = wincode::serialize(&bomb).expect("the bomb should encode");
            assert!(wincode::deserialize_exact::<PersistentPoolElement>(&encoded).is_err());

            let ordinary = pool_element_nested(3);
            let encoded = wincode::serialize(&ordinary).expect("the tree should encode");
            assert!(
                wincode::deserialize_exact::<PersistentPoolElement>(&encoded).is_ok(),
                "an ordinary tree must still decode after a refusal"
            );
        });
    }
}
