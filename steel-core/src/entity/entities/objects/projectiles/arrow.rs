//! Arrow entity.
//!
//! Vanilla parity: `AbstractArrow` and `Arrow`. An arrow flies under gravity and
//! drag, damages what it hits in proportion to its speed, and sticks into the
//! first block it meets until it despawns.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_data::ArrowEntityData;
use steel_registry::{vanilla_damage_types, vanilla_entities, vanilla_items};
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
/// Vanilla parity: `AbstractArrow.baseDamage`, 2.0 for a plain arrow.
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

/// State that is not mirrored to clients.
struct ArrowState {
    /// Damage before the speed multiplier.
    base_damage: f64,
    /// Ticks spent stuck in a block.
    life: i32,
    /// Effects the arrow hands to whatever it hits.
    effects: Vec<MobEffectInstance>,
}

impl ArrowState {
    const fn new() -> Self {
        Self {
            base_damage: DEFAULT_BASE_DAMAGE,
            life: 0,
            effects: Vec::new(),
        }
    }
}

/// A flying arrow.
#[entity_behavior(class = "Arrow")]
pub struct ArrowEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<ArrowEntityData>,
    projectile_base: ProjectileBase,
    state: SyncMutex<ArrowState>,
}

// SAFETY: This Steel-owned key uniquely identifies `ArrowEntity`.
unsafe impl DowncastType for ArrowEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/arrow");
}

impl ArrowEntity {
    /// Creates an arrow at `position`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(ArrowEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(ArrowState::new()),
        }
    }

    /// Creates an arrow from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(ArrowEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(ArrowState::new()),
        }
    }

    /// Spawns an arrow shot by `shooter` and adds it to the world.
    pub fn shoot_from(
        world: &Arc<World>,
        shooter: &dyn Entity,
        entity_type: EntityTypeRef,
        power: f32,
        uncertainty: f32,
    ) -> Arc<Self> {
        let position = shooter.position().with_y(shooter.get_eye_y() - 0.1);
        let arrow = Arc::new(Self::new(
            entity_type,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        arrow.set_owner_uuid(Some(shooter.uuid()));
        let (yaw, pitch) = shooter.rotation();
        arrow.shoot_from_rotation(shooter, pitch, yaw, 0.0, power, uncertainty);

        if let Err(error) = world.try_add_entity(Arc::clone(&arrow) as Arc<dyn Entity>) {
            log::error!("failed to add arrow entity: {error}");
        }
        arrow
    }

    /// Spawns an arrow aimed at `target` rather than along the shooter's look.
    ///
    /// Vanilla parity: the aiming maths of `AbstractSkeleton.performRangedAttack`,
    /// which lifts the shot by a fifth of the horizontal distance so the arc lands
    /// on target.
    pub fn shoot_at(
        world: &Arc<World>,
        shooter: &dyn Entity,
        target: DVec3,
        power: f32,
        uncertainty: f32,
    ) -> Arc<Self> {
        let position = shooter.position().with_y(shooter.get_eye_y() - 0.1);
        let arrow = Arc::new(Self::new(
            &vanilla_entities::ARROW,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        arrow.set_owner_uuid(Some(shooter.uuid()));

        let dx = target.x - position.x;
        let dz = target.z - position.z;
        let horizontal = dx.hypot(dz);
        let dy = horizontal.mul_add(0.2, target.y - position.y);
        arrow.shoot(DVec3::new(dx, dy, dz), power, uncertainty);

        if let Err(error) = world.try_add_entity(Arc::clone(&arrow) as Arc<dyn Entity>) {
            log::error!("failed to add arrow entity: {error}");
        }
        arrow
    }

    /// Adds an effect the arrow will apply to whatever it hits.
    ///
    /// Vanilla parity: `Arrow.addEffect`. Vanilla keeps the effect in the
    /// arrow's `PotionContents` component and scales its duration by
    /// `POTION_DURATION_SCALE`; Steel has no potion component on projectiles
    /// yet, so effects are held on the entity and applied at full duration.
    /// That matches the mobs that add effects directly, such as a stray's
    /// slowness, and only diverges for the tipped arrows Steel cannot fire yet.
    pub fn add_effect(&self, effect: MobEffectInstance) {
        self.state.lock().effects.push(effect);
    }

    /// Hands the arrow's effects to a living target.
    ///
    /// Vanilla parity: `Arrow.doPostHurtEffects`.
    fn do_post_hurt_effects(&self, target: &SharedEntity) {
        let Some(living) = target.as_living_entity() else {
            return;
        };
        // Cloned out of the lock: applying an effect reaches back into the world.
        let effects = self.state.lock().effects.clone();
        for effect in effects {
            living.add_mob_effect(effect);
        }
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
        let yaw = movement.x.atan2(movement.z).to_degrees() as f32;
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

impl Entity for ArrowEntity {
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

    /// Lets a player collect an arrow that has landed.
    ///
    /// Vanilla parity: `AbstractArrow.playerTouch`. Only a stuck arrow can be
    /// picked up, which is why one still in flight passes straight through.
    fn player_touch(self: Arc<Self>, player: &Arc<Player>) {
        if !self.is_in_ground() {
            return;
        }

        // TODO: honor the arrow's pickup mode, so creative-only and
        // non-collectable arrows behave as in vanilla.
        let mut stack = ItemStack::new(&vanilla_items::ARROW);
        let before = stack.count();
        player.inventory.lock().add(&mut stack);
        if stack.count() == before {
            return;
        }

        self.set_removed(RemovalReason::Discarded);
    }
}

impl Projectile for ArrowEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Damages the entity in proportion to the arrow's speed.
    ///
    /// Vanilla parity: `AbstractArrow.onHitEntity`. Damage is
    /// `ceil(speed * base_damage)`, so a fully drawn bow hurts far more than a
    /// spent arrow drifting to the ground.
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
