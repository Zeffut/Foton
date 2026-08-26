//! Spectral arrow entity.
//!
//! Vanilla parity: `SpectralArrow`. An `AbstractArrow` that flies exactly like a
//! plain one and hands its target ten seconds of glowing on top of the damage.
//! That is the whole class: a duration, a `doPostHurtEffects` override, and a
//! different pickup item.
//!
//! The flight body mirrors [`super::ArrowEntity`] the way vanilla's two
//! `AbstractArrow` subclasses mirror each other; `ThrownTridentEntity` already
//! carries its own copy for the same reason.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_data::SpectralArrowEntityData;
use steel_registry::{vanilla_damage_types, vanilla_entities, vanilla_items, vanilla_mob_effects};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, MobEffectInstance, Projectile,
    ProjectileBase, RemovalReason, SharedEntity, next_entity_id,
};
use crate::inventory::container::Container as _;
use crate::physics::MoverType;
use crate::player::Player;
use crate::world::{ClipHitResult, World};

/// Damage an arrow carries before speed is taken into account.
///
/// Vanilla parity: `AbstractArrow.baseDamage`, which `SpectralArrow` leaves at
/// the 2.0 an arrow is built with.
const DEFAULT_BASE_DAMAGE: f64 = 2.0;

/// Vanilla parity: `AbstractArrow.getDefaultGravity`.
const DEFAULT_GRAVITY: f64 = 0.05;

/// Velocity kept each tick in air.
///
/// Vanilla parity: `AbstractArrow.getAirDrag`.
const AIR_DRAG: f64 = 0.99;

/// Ticks a stuck arrow survives before it disappears.
///
/// Vanilla parity: the 1200-tick limit in `AbstractArrow.tickDespawn`.
const DESPAWN_TICKS: i32 = 1200;

/// Ticks of glowing a spectral arrow hands out.
///
/// Vanilla parity: `SpectralArrow.DEFAULT_DURATION`.
const DEFAULT_GLOWING_TICKS: i32 = 200;

/// NBT key the glow duration is stored under.
///
/// Vanilla parity: the `"Duration"` of `SpectralArrow.addAdditionalSaveData`.
const DURATION_NBT_KEY: &str = "Duration";

/// State that is not mirrored to clients.
struct SpectralArrowState {
    /// Damage before the speed multiplier.
    base_damage: f64,
    /// Ticks spent stuck in a block.
    life: i32,
    /// Ticks of glowing this arrow grants.
    duration: i32,
}

impl SpectralArrowState {
    const fn new() -> Self {
        Self {
            base_damage: DEFAULT_BASE_DAMAGE,
            life: 0,
            duration: DEFAULT_GLOWING_TICKS,
        }
    }
}

/// A flying spectral arrow.
#[entity_behavior(class = "SpectralArrow")]
pub struct SpectralArrowEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<SpectralArrowEntityData>,
    projectile_base: ProjectileBase,
    state: SyncMutex<SpectralArrowState>,
}

// SAFETY: This Steel-owned key uniquely identifies `SpectralArrowEntity`.
unsafe impl DowncastType for SpectralArrowEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/spectral_arrow");
}

impl SpectralArrowEntity {
    /// Creates a spectral arrow at `position`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(SpectralArrowEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(SpectralArrowState::new()),
        }
    }

    /// Creates a spectral arrow from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(SpectralArrowEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(SpectralArrowState::new()),
        }
    }

    /// Spawns a spectral arrow shot by `shooter` and adds it to the world.
    ///
    /// Vanilla parity: the `ArrowItem`/`ProjectileWeaponItem` path, which builds
    /// whichever arrow the ammunition declares.
    pub fn shoot_from(
        world: &Arc<World>,
        shooter: &dyn Entity,
        power: f32,
        uncertainty: f32,
    ) -> Arc<Self> {
        let position = shooter.position().with_y(shooter.get_eye_y() - 0.1);
        let arrow = Arc::new(Self::new(
            &vanilla_entities::SPECTRAL_ARROW,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        arrow.set_owner_uuid(Some(shooter.uuid()));
        let (yaw, pitch) = shooter.rotation();
        arrow.shoot_from_rotation(shooter, pitch, yaw, 0.0, power, uncertainty);

        if let Err(error) = world.try_add_entity(Arc::clone(&arrow) as Arc<dyn Entity>) {
            log::error!("failed to add spectral arrow entity: {error}");
        }
        arrow
    }

    /// Returns how long the glowing this arrow grants lasts.
    #[must_use]
    pub fn duration(&self) -> i32 {
        self.state.lock().duration
    }

    /// Sets how long the glowing this arrow grants lasts.
    pub fn set_duration(&self, duration: i32) {
        self.state.lock().duration = duration;
    }

    /// Makes the target glow.
    ///
    /// Vanilla parity: `SpectralArrow.doPostHurtEffects`.
    fn do_post_hurt_effects(&self, target: &SharedEntity) {
        let Some(living) = target.as_living_entity() else {
            return;
        };
        let duration = self.duration();
        living.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::GLOWING,
            duration,
            0,
        ));
    }

    /// Returns whether the arrow is stuck in a block.
    #[must_use]
    pub fn is_in_ground(&self) -> bool {
        *self.entity_data.lock().abstract_arrow.in_ground.get()
    }

    fn set_in_ground(&self, in_ground: bool) {
        self.entity_data
            .lock()
            .abstract_arrow
            .in_ground
            .set(in_ground);
    }

    /// Returns the damage this arrow deals before the speed multiplier.
    #[must_use]
    pub fn base_damage(&self) -> f64 {
        self.state.lock().base_damage
    }

    /// Sets the damage this arrow deals before the speed multiplier.
    pub fn set_base_damage(&self, damage: f64) {
        self.state.lock().base_damage = damage;
    }

    /// Points the arrow along its current velocity.
    ///
    /// Vanilla parity: the rotation block of `AbstractArrow.tick`.
    fn face_velocity(&self) {
        let movement = self.velocity();
        let horizontal = movement.x.hypot(movement.z);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla stores rotations as f32"
        )]
        let yaw = movement.x.atan2(movement.z).to_degrees() as f32;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla stores rotations as f32"
        )]
        let pitch = movement.y.atan2(horizontal).to_degrees() as f32;
        self.set_rotation((yaw, pitch));
    }

    /// Counts down the lifetime of a stuck arrow.
    ///
    /// Vanilla parity: `AbstractArrow.tickDespawn`.
    fn tick_despawn(&self) {
        let expired = {
            let mut state = self.state.lock();
            state.life += 1;
            state.life >= DESPAWN_TICKS
        };
        if expired {
            self.set_removed(RemovalReason::Discarded);
        }
    }
}

impl Entity for SpectralArrowEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        if self.is_in_ground() {
            self.tick_despawn();
            return;
        }

        self.check_left_owner();
        self.face_velocity();

        if let Some(hit) = self.get_hit_result_on_move_vector() {
            self.hit_target_or_deflect_self(&hit);
            if !self.is_alive() {
                return;
            }
        }

        let _ = self.move_entity(MoverType::SelfMovement, self.velocity());
        self.apply_effects_from_blocks();
        self.set_velocity(self.velocity() * AIR_DRAG);
        self.apply_gravity();

        // Vanilla also spawns an `EFFECT` particle each tick, but only on the
        // client (`this.level().isClientSide()`), so there is nothing to send.
    }

    fn get_default_gravity(&self) -> f64 {
        DEFAULT_GRAVITY
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn is_pickable(&self) -> bool {
        true
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert(DURATION_NBT_KEY, self.duration());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        // Vanilla parity: `getIntOr("Duration", 200)`.
        self.set_duration(nbt.int(DURATION_NBT_KEY).unwrap_or(DEFAULT_GLOWING_TICKS));
    }

    /// Lets a player collect an arrow that has landed.
    ///
    /// Vanilla parity: `AbstractArrow.playerTouch` with
    /// `SpectralArrow.getDefaultPickupItem`. A spectral arrow gives back a
    /// spectral arrow, not a plain one.
    fn player_touch(self: Arc<Self>, player: &Arc<Player>) {
        if !self.is_in_ground() {
            return;
        }

        // TODO: honor the arrow's pickup mode, so creative-only and
        // non-collectable arrows behave as in vanilla.
        let mut stack = ItemStack::new(&vanilla_items::SPECTRAL_ARROW);
        let before = stack.count();
        player.inventory.lock().add(&mut stack);
        if stack.count() == before {
            return;
        }

        self.set_removed(RemovalReason::Discarded);
    }
}

impl Projectile for SpectralArrowEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Damages the entity in proportion to the arrow's speed, then makes it
    /// glow.
    ///
    /// Vanilla parity: `AbstractArrow.onHitEntity` plus
    /// `SpectralArrow.doPostHurtEffects`.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let speed = self.velocity().length();
        let damage = (speed * self.base_damage()).ceil().max(0.0);

        let mut source = DamageSource::environment(&vanilla_damage_types::ARROW)
            .with_direct_entity(self.id())
            .with_source_position(self.position());
        if let Some(owner) = self.get_owner() {
            source = source.with_causing_entity(owner.id());
        }

        if let Some(world) = self.level() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "vanilla passes arrow damage as an f32"
            )]
            entity.hurt(&world, &source, damage as f32);
        }

        self.do_post_hurt_effects(entity);

        // TODO: vanilla also sets the target's arrow count, applies piercing,
        // ignites the target when the arrow burns, and runs the weapon's
        // enchantment effects.
        self.set_removed(RemovalReason::Discarded);
    }

    /// Sticks the arrow into the block it hit.
    ///
    /// Vanilla parity: `AbstractArrow.onHitBlock`.
    fn on_hit_block(&self, _hit: &ClipHitResult) {
        // TODO: vanilla snaps the arrow onto the face it struck; Steel has no
        // direct position setter on the entity trait, so it stops where it is.
        self.set_velocity(DVec3::ZERO);
        self.set_in_ground(true);
        self.state.lock().life = 0;
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::init_vanilla_registry;

    use super::*;

    fn spectral_arrow() -> SpectralArrowEntity {
        init_vanilla_registry();
        SpectralArrowEntity::new(
            &vanilla_entities::SPECTRAL_ARROW,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        )
    }

    /// The glow duration is the only state this class owns, and vanilla writes
    /// it under `Duration`. A tipped-down arrow that reloaded at 200 would
    /// silently reset every custom-duration arrow in a world.
    #[test]
    fn a_spectral_arrow_keeps_its_glow_duration_across_a_save() {
        let arrow = spectral_arrow();
        assert_eq!(arrow.duration(), DEFAULT_GLOWING_TICKS);
        arrow.set_duration(45);

        let mut nbt = NbtCompound::new();
        arrow.save_additional(&mut nbt);
        assert_eq!(nbt.int("Duration"), Some(45));

        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let reloaded = spectral_arrow();
        reloaded.load_additional((&borrowed).into());
        assert_eq!(reloaded.duration(), 45);
    }

    /// Vanilla's `getIntOr("Duration", 200)` means an arrow written without the
    /// key comes back at the default rather than at zero.
    #[test]
    fn a_spectral_arrow_saved_without_a_duration_glows_for_the_default() {
        let mut bytes = Vec::new();
        NbtCompound::new().write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let arrow = spectral_arrow();
        arrow.set_duration(1);
        arrow.load_additional((&borrowed).into());
        assert_eq!(arrow.duration(), DEFAULT_GLOWING_TICKS);
    }
}
