//! The nautilus family.
//!
//! Vanilla parity: the `net.minecraft.world.entity.animal.nautilus` package.
//! Both mobs share the [`AbstractNautilus`](crate::entity::AbstractNautilus)
//! layer, so they are grouped the way vanilla groups them rather than split
//! across Steel's passive/hostile folders -- the zombie nautilus is a `MONSTER`
//! by category and still belongs here, the same way the zombie horse sits with
//! the equines.

#[expect(
    clippy::module_inception,
    reason = "vanilla's `animal.nautilus` package holds a `Nautilus` class, and \
              the family folder is named after the family the way `equine` is"
)]
mod nautilus;
mod nautilus_ai;
mod zombie_nautilus;
mod zombie_nautilus_ai;

pub use nautilus::NautilusEntity;
pub use zombie_nautilus::ZombieNautilusEntity;
