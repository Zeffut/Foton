//! Eye of ender entity.
//!
//! Vanilla parity: `EyeOfEnder`. Not a `Projectile` at all despite living in
//! that package -- it is a bare `Entity` that steers itself toward a point,
//! survives eighty ticks, and then either drops back as an item or shatters.
//!
//! The steering is what makes it readable in the sky: the horizontal speed
//! creeps toward the remaining distance a quarter of a percent per tick, so a
//! far-off stronghold makes the eye accelerate away and a near one makes it
//! slow to a hover, and the vertical component eases toward straight up or
//! straight down rather than snapping.
//!
//! **Gap**: `EnderEyeItem.use` throws one of these at the nearest
//! `#minecraft:eye_of_ender_located` structure, found with
//! `ServerLevel.findNearestMapStructure`. Steel's structure search
//! ([`crate::command::builtins`]'s `locate`) is a `CommandResultSuspension`
//! state machine driven by the command scheduler, and a game tick may not wait
//! on chunk generation, so there is nothing an item-use handler can call. The
//! entity is complete and [`EyeOfEnderEntity::signal_to`] is its whole
//! interface; only the search that supplies the target is missing.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::level_events::PARTICLES_EYE_OF_ENDER_DEATH;
use steel_registry::vanilla_entity_data::EyeOfEnderEntityData;
use steel_registry::{sound_events, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntitySyncedData, RemovalReason};
use crate::world::World;

/// How far ahead of itself the eye will aim in one hop.
///
/// Vanilla parity: `EyeOfEnder.TOO_FAR_DISTANCE`. A target further than this is
/// replaced by a waypoint twelve blocks along the bearing, which is why an eye
/// thrown at a stronghold a thousand blocks away still arcs visibly rather than
/// shooting off at the horizon.
const TOO_FAR_DISTANCE: f64 = 12.0;

/// How high the eye climbs when it aims at a waypoint rather than the target.
///
/// Vanilla parity: `EyeOfEnder.TOO_FAR_SIGNAL_HEIGHT`.
const TOO_FAR_SIGNAL_HEIGHT: f64 = 8.0;

/// Ticks the eye survives.
///
/// Vanilla parity: the `this.life > 80` of `EyeOfEnder.tick`.
const LIFETIME_TICKS: i32 = 80;

/// One in this many eyes shatters instead of dropping.
///
/// Vanilla parity: the `random.nextInt(5) > 0` of `signalTo`, so four in five
/// survive.
const SURVIVAL_ROLL: i32 = 5;

/// How fast the horizontal speed chases the remaining distance.
///
/// Vanilla parity: the `Mth.lerp(0.0025, ...)` of `updateDeltaMovement`.
const SPEED_LERP: f64 = 0.0025;

/// Distance below which the eye starts braking.
///
/// Vanilla parity: the `horizontalLength < 1.0` branch.
const BRAKING_DISTANCE: f64 = 1.0;

/// Fraction of speed kept each tick while braking.
const BRAKING_FACTOR: f64 = 0.8;

/// How fast the vertical speed eases toward its wanted value.
///
/// Vanilla parity: the `* 0.015` of `updateDeltaMovement`.
const VERTICAL_LERP: f64 = 0.015;

/// NBT key the carried item is stored under.
///
/// Vanilla parity: the `output.store("Item", ...)` of `addAdditionalSaveData`.
const ITEM_NBT_KEY: &str = "Item";

/// State the eye keeps that is not mirrored to clients.
struct EyeState {
    /// Where the eye is steering, once it has been signaled.
    target: Option<DVec3>,
    /// Ticks since it was signaled.
    life: i32,
    /// Whether it drops as an item rather than shattering.
    survive_after_death: bool,
}

impl EyeState {
    const fn new() -> Self {
        Self {
            target: None,
            life: 0,
            survive_after_death: false,
        }
    }
}

/// A thrown eye of ender.
#[entity_behavior(class = "EyeOfEnder")]
pub struct EyeOfEnderEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<EyeOfEnderEntityData>,
    state: SyncMutex<EyeState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies
// `EyeOfEnderEntity`.
unsafe impl DowncastType for EyeOfEnderEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/eye_of_ender");
}

impl EyeOfEnderEntity {
    /// Creates an eye of ender at `position`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(EyeOfEnderEntityData::new()),
            state: SyncMutex::new(EyeState::new()),
        }
    }

    /// Creates an eye of ender from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(EyeOfEnderEntityData::new()),
            state: SyncMutex::new(EyeState::new()),
        }
    }

    /// Sets the item the eye shows and drops.
    ///
    /// Vanilla parity: `EyeOfEnder.setItem`, which falls back to a plain ender
    /// eye when handed an empty stack.
    pub fn set_item(&self, item: ItemStack) {
        let stored = if item.is_empty() {
            ItemStack::new(&vanilla_items::ENDER_EYE)
        } else {
            item.copy_with_count(1)
        };
        self.entity_data.lock().item_stack.set(stored);
    }

    /// Returns the item the eye shows and drops.
    #[must_use]
    pub fn get_item(&self) -> ItemStack {
        self.entity_data.lock().item_stack.get().clone()
    }

    /// Points the eye at `target` and starts its clock.
    ///
    /// Vanilla parity: `EyeOfEnder.signalTo`. A target more than
    /// [`TOO_FAR_DISTANCE`] away horizontally is replaced by a waypoint that far
    /// along the same bearing and eight blocks up, so the eye climbs and flies
    /// off rather than aiming at a point it can never reach in eighty ticks.
    pub fn signal_to(&self, target: DVec3) {
        self.signal_to_with_survival(target, rand::random_range(0..SURVIVAL_ROLL) > 0);
    }

    /// [`Self::signal_to`] with the survival roll supplied rather than rolled.
    fn signal_to_with_survival(&self, target: DVec3, survive_after_death: bool) {
        let position = self.position();
        let delta = target - position;
        let horizontal_distance = delta.x.hypot(delta.z);

        let aim = if horizontal_distance > TOO_FAR_DISTANCE {
            position
                + DVec3::new(
                    delta.x / horizontal_distance * TOO_FAR_DISTANCE,
                    TOO_FAR_SIGNAL_HEIGHT,
                    delta.z / horizontal_distance * TOO_FAR_DISTANCE,
                )
        } else {
            target
        };

        let mut state = self.state.lock();
        state.target = Some(aim);
        state.life = 0;
        state.survive_after_death = survive_after_death;
    }

    /// Returns where the eye is steering, if it has been signaled.
    #[must_use]
    pub fn target(&self) -> Option<DVec3> {
        self.state.lock().target
    }

    /// Returns the velocity the eye should carry next tick.
    ///
    /// Vanilla parity: `EyeOfEnder.updateDeltaMovement`, kept as a free function
    /// for the same reason vanilla keeps it static: it is pure, and it is the
    /// only part of the entity worth testing on its own.
    #[must_use]
    fn steer(old_movement: DVec3, position: DVec3, target: DVec3) -> DVec3 {
        let horizontal_delta = DVec3::new(target.x - position.x, 0.0, target.z - position.z);
        let horizontal_length = horizontal_delta.length();
        let old_horizontal = old_movement.x.hypot(old_movement.z);
        let mut wanted_speed =
            SPEED_LERP.mul_add(horizontal_length - old_horizontal, old_horizontal);
        let mut movement_y = old_movement.y;
        if horizontal_length < BRAKING_DISTANCE {
            wanted_speed *= BRAKING_FACTOR;
            movement_y *= BRAKING_FACTOR;
        }

        // Vanilla compares the target height against `y - deltaMovement.y`, not
        // against `y`. Keeping that quirk is what makes the eye overshoot and
        // settle rather than converge cleanly.
        let wanted_movement_y = if position.y - old_movement.y < target.y {
            1.0
        } else {
            -1.0
        };

        horizontal_delta * (wanted_speed / horizontal_length)
            + DVec3::new(
                0.0,
                (wanted_movement_y - movement_y).mul_add(VERTICAL_LERP, movement_y),
                0.0,
            )
    }

    /// Ends the eye's flight.
    ///
    /// Vanilla parity: the `life > 80` branch of `EyeOfEnder.tick`.
    fn expire(&self, world: &Arc<World>) {
        world.play_sound_at(
            &sound_events::ENTITY_ENDER_EYE_DEATH,
            SoundSource::Neutral,
            self.position(),
            1.0,
            1.0,
            None,
        );
        let survives = self.state.lock().survive_after_death;
        let item = self.get_item();
        let position = self.position();
        self.set_removed(RemovalReason::Discarded);

        if survives {
            world.spawn_item(position, item);
        } else {
            world.level_event(PARTICLES_EYE_OF_ENDER_DEATH, self.block_position(), 0, None);
        }
    }
}

impl Entity for EyeOfEnderEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `EyeOfEnder.tick`.
    fn tick(&self) {
        self.base_tick();

        let Some(world) = self.level() else {
            return;
        };

        let new_position = self.position() + self.velocity();
        if let Some(target) = self.target() {
            self.set_velocity(Self::steer(self.velocity(), new_position, target));
        }
        // Vanilla assigns straight through `setPos`: the eye passes through
        // blocks, which is what lets it lead a player over terrain.
        if let Err(error) = self.base.try_set_position(new_position) {
            log::debug!("eye of ender {} could not move: {error}", self.base.id());
            return;
        }

        let expired = {
            let mut state = self.state.lock();
            state.life += 1;
            state.life > LIFETIME_TICKS
        };
        if expired {
            self.expire(&world);
        }
    }

    /// Vanilla parity: `EyeOfEnder` does not override `getDefaultGravity`, so it
    /// falls under nothing and flies purely on its steering.
    fn get_default_gravity(&self) -> f64 {
        0.0
    }

    /// Vanilla parity: `EyeOfEnder.isAttackable`.
    fn attackable(&self) -> bool {
        false
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert(ITEM_NBT_KEY, self.get_item().to_nbt_tag_ref());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        // Vanilla parity: `readAdditionalSaveData` falls back to the default
        // item when the stored one will not parse.
        let item = nbt
            .compound(ITEM_NBT_KEY)
            .and_then(|tag| ItemStack::from_borrowed_compound(&tag))
            .unwrap_or_else(|| ItemStack::new(&vanilla_items::ENDER_EYE));
        self.set_item(item);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    use super::*;
    use crate::entity::next_entity_id;

    fn eye() -> EyeOfEnderEntity {
        init_vanilla_registry();
        EyeOfEnderEntity::new(
            &vanilla_entities::EYE_OF_ENDER,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        )
    }

    /// A stronghold is hundreds of blocks away, and vanilla does not aim at it:
    /// it aims twelve blocks along the bearing and eight blocks up. Aiming at
    /// the real target would send the eye off flat and low.
    #[test]
    fn a_far_target_is_replaced_by_a_waypoint_twelve_out_and_eight_up() {
        let entity = eye();
        entity.signal_to(DVec3::new(8.5 + 400.0, 64.0, 8.5));

        let Some(aim) = entity.target() else {
            panic!("signal_to must set a target");
        };
        assert!(
            (aim.x - (8.5 + TOO_FAR_DISTANCE)).abs() < 1e-9,
            "aim.x = {}",
            aim.x
        );
        assert!((aim.z - 8.5).abs() < 1e-9, "aim.z = {}", aim.z);
        assert!(
            (aim.y - (64.0 + TOO_FAR_SIGNAL_HEIGHT)).abs() < 1e-9,
            "aim.y = {}",
            aim.y
        );
    }

    /// Inside twelve blocks the eye aims at the real point, unchanged. The two
    /// arms of `signalTo` are the whole method, so a test that only covered the
    /// far one would leave the near one free to be wrong.
    #[test]
    fn a_near_target_is_aimed_at_exactly() {
        let entity = eye();
        let target = DVec3::new(12.5, 70.0, 8.5);
        entity.signal_to(target);
        assert_eq!(entity.target(), Some(target));
    }

    /// The horizontal speed chases the remaining distance rather than jumping
    /// to it: a quarter of a percent of the gap per tick. An eye that snapped to
    /// the full distance would cross a stronghold's worth of ground in one tick.
    #[test]
    fn the_horizontal_speed_creeps_toward_the_remaining_distance() {
        let position = DVec3::new(0.0, 64.0, 0.0);
        let target = DVec3::new(100.0, 64.0, 0.0);
        let stepped = EyeOfEnderEntity::steer(DVec3::ZERO, position, target);

        let expected = SPEED_LERP * 100.0;
        assert!(
            (stepped.x - expected).abs() < 1e-9,
            "expected {expected}, got {}",
            stepped.x
        );
    }

    /// Inside a block of the target both the speed *and* the climb are cut to
    /// four fifths, which is what makes the eye hover instead of orbiting.
    ///
    /// Both components are asserted against their exact values rather than
    /// against an unbraked run at a different target: the speed lerp already
    /// makes a nearer target slower, so a comparison between two targets stays
    /// green with the braking branch deleted.
    #[test]
    fn the_eye_brakes_within_a_block_of_its_target() {
        let position = DVec3::new(0.0, 64.0, 0.0);
        let target = DVec3::new(0.5, 64.0, 0.0);
        let old = DVec3::new(0.5, 0.5, 0.0);

        let braked = EyeOfEnderEntity::steer(old, position, target);

        // Half a block out with half a block per tick of speed: the lerp is a
        // no-op, so the whole of `0.5 -> 0.4` is the braking factor.
        let expected_x = 0.5 * BRAKING_FACTOR;
        assert!(
            (braked.x - expected_x).abs() < 1e-12,
            "horizontal speed must be braked to {expected_x}, got {}",
            braked.x
        );

        // The climb is braked first and only then eased toward its wanted +1.
        let expected_y = (1.0 - 0.5 * BRAKING_FACTOR).mul_add(VERTICAL_LERP, 0.5 * BRAKING_FACTOR);
        assert!(
            (braked.y - expected_y).abs() < 1e-12,
            "vertical speed must be braked to {expected_y}, got {}",
            braked.y
        );
    }

    /// The carried item is what the eye drops when it survives, so it has to
    /// survive a save. Vanilla stores it under `Item`.
    #[test]
    fn the_carried_item_round_trips_through_nbt() {
        let entity = eye();
        entity.set_item(ItemStack::new(&vanilla_items::DIAMOND));

        let mut nbt = NbtCompound::new();
        entity.save_additional(&mut nbt);

        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let reloaded = eye();
        reloaded.load_additional((&borrowed).into());
        assert!(reloaded.get_item().is(&vanilla_items::DIAMOND));
    }

    /// Vanilla's `setItem` swaps an empty stack for a plain ender eye rather
    /// than showing nothing, and clamps the count to one.
    #[test]
    fn an_empty_item_becomes_a_plain_ender_eye() {
        let entity = eye();
        entity.set_item(ItemStack::empty());
        assert!(entity.get_item().is(&vanilla_items::ENDER_EYE));

        entity.set_item(ItemStack::with_count(&vanilla_items::ENDER_EYE, 7));
        assert_eq!(entity.get_item().count(), 1);
    }
}
