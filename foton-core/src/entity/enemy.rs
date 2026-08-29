//! The marker every hostile mob carries.

use crate::entity::LivingEntity;

/// Marks a mob that golems, and vanilla's other defenders, treat as hostile.
///
/// Vanilla parity: `net.minecraft.world.entity.monster.Enemy`. Vanilla puts it
/// on `Monster` and on the handful of hostiles that sit outside that hierarchy
/// (slimes, ghasts, phantoms, shulkers, hoglins, the ender dragon), and reads
/// it back with `instanceof Enemy`; Foton reads it back with
/// [`crate::entity::Entity::as_enemy`].
pub trait Enemy: LivingEntity {}
