//! Turning one mob into another in place.
//!
//! Vanilla parity: `Mob.convertTo`, `ConversionParams` and `ConversionType`.
//! This is the foundation behind every "becomes a" in the game -- a villager
//! struck by lightning becoming a witch, a zombie drowning into a drowned, and
//! the villager/zombie-villager pair that cure farms are built on.
//!
//! Foton builds the replacement through a caller-supplied constructor rather
//! than vanilla's `EntityType.create`, because the caller always knows the
//! concrete type it wants and that keeps the after-conversion callback typed
//! without a downcast.

use std::sync::{Arc, Weak};

use glam::DVec3;

use crate::entity::callback::RemovalReason;
use crate::entity::{Entity, EntityBase, Mob, SharedEntity, next_entity_id};
use crate::event::EntityTransformEvent;
use crate::world::World;

/// Whether the mob being converted is replaced or merely spawns something.
///
/// Vanilla parity: `net.minecraft.world.entity.ConversionType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionType {
    /// The old mob is replaced: its position, ride, equipment and leash all
    /// move across, and it is discarded afterwards.
    Single,
    /// The old mob spawns the new one and stays put -- a slime splitting.
    SplitOnDeath,
}

/// Why vanilla replaced an entity. Kept on the conversion so Bukkit can expose
/// the same reason instead of guessing from the source and target types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionReason {
    /// A transition whose producer is not yet classified.
    Unknown,
    /// A golden apple and weakness -- a zombie villager becoming a villager.
    Cured,
    /// Held under water -- a zombie becoming a drowned.
    Drowned,
    /// Left in powder snow -- a skeleton becoming a stray.
    Frozen,
    /// A zombie infecting a villager.
    Infection,
    /// Struck by lightning -- a pig becoming a zombified piglin, a villager a
    /// witch.
    Lightning,
    /// Carried into the overworld -- a piglin becoming a zombified piglin.
    PiglinZombification,
    /// Poisoned -- a mooshroom changing variant.
    Poison,
    /// A slime or magma cube splitting on death.
    Split,
}

impl ConversionType {
    /// Vanilla parity: `ConversionType.shouldDiscardAfterConversion`.
    #[must_use]
    pub const fn should_discard_after_conversion(self) -> bool {
        matches!(self, Self::Single)
    }
}

/// How much of the old mob the new one inherits.
///
/// Vanilla parity: `net.minecraft.world.entity.ConversionParams`.
#[derive(Debug, Clone, Copy)]
pub struct ConversionParams {
    /// Whether the old mob is replaced or merely spawns the new one.
    pub conversion_type: ConversionType,
    /// Whether armor and held items move across.
    ///
    /// The move itself is the caller's, because only a caller with both
    /// concrete types can name the slots on each side. Vanilla parity: the
    /// `keepEquipment` block of `ConversionType.SINGLE.convert`.
    pub keep_equipment: bool,
    /// Whether the new mob inherits `canPickUpLoot`.
    pub preserve_can_pick_up_loot: bool,
    /// Vanilla-facing reason reported by `EntityTransformEvent`.
    pub reason: ConversionReason,
    // MISSING FOUNDATION: vanilla's fourth field is the old mob's scoreboard
    // `PlayerTeam`, which the conversion re-registers the new mob into. Foton
    // has no scoreboard teams, so a converted mob simply has no team to keep.
}

impl ConversionParams {
    /// Vanilla parity: `ConversionParams.single`.
    #[must_use]
    pub const fn single(keep_equipment: bool, preserve_can_pick_up_loot: bool) -> Self {
        Self {
            conversion_type: ConversionType::Single,
            keep_equipment,
            preserve_can_pick_up_loot,
            reason: ConversionReason::Unknown,
        }
    }

    /// Attaches the vanilla cause reported by the transform event.
    #[must_use]
    pub const fn with_reason(mut self, reason: ConversionReason) -> Self {
        self.reason = reason;
        self
    }
}

/// Replaces `from` with a new mob built by `build`.
///
/// Vanilla parity: `Mob.convertTo(EntityType, ConversionParams, AfterConversion)`.
/// The order matters and is vanilla's: the state is copied, then the caller's
/// `after` runs, then the new mob joins the world, and only then is the old one
/// discarded -- so nothing ever sees a moment with both of them live, or with
/// neither.
///
/// Returns `None` when the old mob is already removed or has no world, matching
/// vanilla's null returns; the caller then leaves it alone.
pub fn convert_to<T, B, A>(
    from: &dyn Mob,
    params: ConversionParams,
    build: B,
    after: A,
) -> Option<Arc<T>>
where
    T: Mob + 'static,
    B: FnOnce(i32, DVec3, Weak<World>) -> T,
    A: FnOnce(&Arc<T>),
{
    if from.is_removed() {
        return None;
    }
    let world = from.level()?;

    let converted = Arc::new(build(
        next_entity_id(),
        from.position(),
        Arc::downgrade(&world),
    ));
    copy_common_state(from, converted.as_ref(), params);

    after(&converted);

    let mut event = EntityTransformEvent::new(from.uuid(), converted.uuid(), params.reason);
    world.fire_event(&mut event);
    if event.is_cancelled() {
        return None;
    }

    if world
        .try_add_entity(Arc::clone(&converted) as SharedEntity)
        .is_err()
    {
        // The chunk went away between the check and the add. Leave the old mob
        // alone rather than discarding it in favor of one that never joined.
        return None;
    }

    if let Some(source) = world.get_entity_by_id(from.id()) {
        EntityBase::transfer_relationships(&source, &(Arc::clone(&converted) as SharedEntity));
    }

    if params.conversion_type.should_discard_after_conversion() {
        from.set_removed(RemovalReason::Discarded);
    }

    Some(converted)
}

/// Replaces a non-mob entity while preserving its shared riding relationships.
///
/// This is intentionally narrower than mob conversion: callers own any
/// type-specific state, while the world insertion/removal and relationship
/// ordering are handled centrally.
pub fn replace_entity<T, B>(
    from: &SharedEntity,
    reason: ConversionReason,
    build: B,
) -> Option<Arc<T>>
where
    T: Entity + 'static,
    B: FnOnce(i32, DVec3, Weak<World>) -> T,
{
    if from.is_removed() {
        return None;
    }
    let world = from.level()?;
    let converted = Arc::new(build(
        next_entity_id(),
        from.position(),
        Arc::downgrade(&world),
    ));
    converted.set_velocity(from.velocity());
    converted.set_rotation(from.rotation());
    converted.set_on_ground(from.on_ground());
    converted.set_fall_distance(from.fall_distance());
    let mut event = EntityTransformEvent::new(from.uuid(), converted.uuid(), reason);
    world.fire_event(&mut event);
    if event.is_cancelled() {
        return None;
    }
    if world
        .try_add_entity(Arc::clone(&converted) as SharedEntity)
        .is_err()
    {
        return None;
    }
    EntityBase::transfer_relationships(from, &(Arc::clone(&converted) as SharedEntity));
    from.set_removed(RemovalReason::Discarded);
    Some(converted)
}

/// Copies the state every conversion carries across.
///
/// Vanilla parity: `ConversionType.convertCommon`, plus the position, motion
/// and rotation half of `ConversionType.SINGLE.convert`. The new mob is built
/// at the old one's position, so only the rest is copied here.
fn copy_common_state(from: &dyn Mob, to: &dyn Mob, params: ConversionParams) {
    if params.conversion_type == ConversionType::Single {
        to.set_velocity(from.velocity());
        to.set_rotation(from.rotation());
        to.set_y_body_rot(from.y_body_rot());
        to.set_on_ground(from.on_ground());
        to.set_fall_distance(from.fall_distance());
    }

    to.set_absorption_amount(from.living_base().absorption_amount());
    for effect in from.living_base().active_mob_effects() {
        to.living_base().add_mob_effect(effect);
    }

    // Vanilla parity: the baby flag and, for two ageable mobs, the whole age
    // clock. Without this no conversion carried age at all: a baby hoglin came
    // back from the overworld as a grown zoglin, and a cured zombie villager
    // lost however far along its child was.
    if from.is_baby() {
        to.set_baby(true);
    }
    if let (Some(old_ageable), Some(new_ageable)) = (from.as_ageable_mob(), to.as_ageable_mob()) {
        new_ageable.set_age(old_ageable.get_age());
        new_ageable.set_forced_age(old_ageable.forced_age());
        new_ageable.set_forced_age_timer(old_ageable.forced_age_timer());
    }

    if params.preserve_can_pick_up_loot {
        to.set_can_pick_up_loot(*from.mob_base().can_pick_up_loot().lock());
    }
    to.set_left_handed(from.is_left_handed());
    to.set_no_ai(from.is_no_ai());
    if from.is_persistence_required() {
        to.set_persistence_required();
    }

    to.set_custom_name(from.custom_name());
    to.set_custom_name_visible(from.is_custom_name_visible());
    to.set_remaining_fire_ticks(from.remaining_fire_ticks());
    to.set_invulnerable(from.is_invulnerable());
    to.set_no_gravity(from.is_no_gravity());
    to.set_portal_cooldown(from.portal_cooldown());
    to.set_silent(from.is_silent());
    for tag in from.tags() {
        to.add_tag(tag);
    }

    // MISSING FOUNDATION: vanilla also carries the passenger and the leash
    // across, copies `CUSTOM_DATA`, moves the `ANGRY_AT` brain memory, and
    // re-registers the scoreboard team. Foton has no scoreboard teams, and the
    // rest are not reachable from a `&dyn Mob` yet. None of them are load
    // bearing for the pairs this serves today: neither a villager nor a piglin
    // can be leashed, and a converting piglin has already had its anger erased
    // by `PiglinAi.cancelAdmiring`.
}
