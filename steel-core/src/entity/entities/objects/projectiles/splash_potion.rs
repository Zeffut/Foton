//! Thrown splash potion.
//!
//! Vanilla parity: `AbstractThrownPotion` and `ThrownSplashPotion`. Brewing
//! could already turn a potion into a splash potion with gunpowder, and a
//! player could hold one and do nothing with it. This is what the gunpowder was
//! for.
//!
//! The distance falloff is the whole reason splash potions are a choice rather
//! than a strictly better bottle: full strength only at the center, nothing
//! past four blocks, and a duration cut in proportion.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::data_components::vanilla_components::POTION_CONTENTS;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::level_events::{
    PARTICLES_INSTANT_POTION_SPLASH, PARTICLES_SPELL_POTION_SPLASH,
};
use steel_registry::vanilla_entity_data::SplashPotionEntityData;
use steel_registry::{vanilla_damage_types, vanilla_entities, vanilla_items, vanilla_potions};
use steel_utils::locks::SyncMutex;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey, WorldAabb};

use crate::behavior::potion_effects;
use crate::entity::damage::DamageSource;
use crate::entity::entities::{AreaEffectCloudEntity, AxolotlEntity};
use crate::entity::next_entity_id;
use crate::entity::projectile::{
    Projectile, ProjectileBase, ProjectileHit, ThrowableItemProjectile, ThrowableProjectile,
};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, MobEffectInstance, RemovalReason,
    SharedEntity,
};
use crate::world::World;

/// How far a splash reaches.
///
/// Vanilla parity: `AbstractThrownPotion.SPLASH_RANGE`.
const SPLASH_RANGE: f64 = 4.0;

/// Squared splash range, as vanilla compares it.
const SPLASH_RANGE_SQR: f64 = SPLASH_RANGE * SPLASH_RANGE;

/// How far up and down the splash box reaches.
///
/// Vanilla parity: the `inflate(4.0, 2.0, 4.0)` of `onHitAsPotion`. A splash is
/// wider than it is tall, so one thrown at your feet catches you and one thrown
/// two floors up does not.
const SPLASH_HEIGHT: f64 = 2.0;

/// Damage a water bottle does to something that hates water.
///
/// Vanilla parity: the `1.0F` of `onHitAsWater`.
const WATER_DAMAGE: f32 = 1.0;

/// Shortest effect a splash is allowed to grant.
///
/// Vanilla parity: the `endsWithin(20)` guard. An effect diluted to under a
/// second is dropped rather than flickering on and straight back off.
const MINIMUM_EFFECT_TICKS: i32 = 20;

/// A thrown splash potion.
#[entity_behavior(class = "ThrownSplashPotion")]
pub struct SplashPotionEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<SplashPotionEntityData>,
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SplashPotionEntity`.
unsafe impl DowncastType for SplashPotionEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/splash_potion");
}

impl SplashPotionEntity {
    /// Creates a thrown splash potion with no owner.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(SplashPotionEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a thrown splash potion from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(SplashPotionEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Returns everything within reach of the burst.
    fn entities_in_splash(&self, world: &Arc<World>) -> Vec<SharedEntity> {
        let position = self.position();
        let aabb = WorldAabb::new(
            position.x - SPLASH_RANGE,
            position.y - SPLASH_HEIGHT,
            position.z - SPLASH_RANGE,
            position.x + SPLASH_RANGE,
            position.y + SPLASH_HEIGHT,
            position.z + SPLASH_RANGE,
        );
        world.get_entities_in_aabb(&aabb)
    }

    /// Douses what a water bottle lands near.
    ///
    /// Vanilla parity: `AbstractThrownPotion.onHitAsWater`. A water bottle is
    /// the one splash that is useful for what it does not contain: it puts out
    /// burning mobs and burns the ones that hate water, the enderman among
    /// them.
    fn splash_as_water(&self, world: &Arc<World>) {
        let position = self.position();
        for entity in self.entities_in_splash(world) {
            if entity.position().distance_squared(position) >= SPLASH_RANGE_SQR {
                continue;
            }
            let Some(living) = entity.as_living_entity() else {
                continue;
            };

            if living.is_sensitive_to_water() {
                let mut damage = DamageSource::environment(&vanilla_damage_types::INDIRECT_MAGIC)
                    .with_direct_entity(self.id());
                if let Some(owner) = self.get_owner() {
                    damage = damage.with_causing_entity(owner.id());
                }
                entity.hurt(world, &damage, WATER_DAMAGE);
            }

            if entity.is_on_fire() && Entity::is_alive(entity.as_ref()) {
                entity.clear_fire();
            }
        }

        // Vanilla parity: the second loop of `onHitAsWater`, which has no
        // distance test of its own -- an axolotl anywhere in the splash box
        // gets its air back, even one further out than the dousing reaches.
        for entity in self.entities_in_splash(world) {
            if let Some(axolotl) = entity.downcast_ref::<AxolotlEntity>() {
                axolotl.rehydrate();
            }
        }
    }

    /// Returns whether this bottle leaves a cloud instead of splashing once.
    fn lingers(&self) -> bool {
        self.get_item().is(&vanilla_items::LINGERING_POTION)
    }

    /// Leaves a lingering cloud where the bottle broke.
    ///
    /// Vanilla parity: `ThrownLingeringPotion.onHitAsPotion`.
    fn leave_cloud(&self, world: &Arc<World>, potion: &ItemStack) {
        let Some(contents) = potion.get(POTION_CONTENTS) else {
            return;
        };
        let effects = potion_effects(contents);
        if effects.is_empty() {
            return;
        }

        let cloud = Arc::new(AreaEffectCloudEntity::new(
            &vanilla_entities::AREA_EFFECT_CLOUD,
            next_entity_id(),
            self.position(),
            Arc::downgrade(world),
        ));
        cloud.configure_as_lingering(effects);

        let entity: SharedEntity = cloud;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("failed to spawn area effect cloud: {error}");
        }
    }

    /// Applies the potion to everything in reach, weaker the further out.
    ///
    /// Vanilla parity: `ThrownSplashPotion.onHitAsPotion`.
    fn splash_as_potion(&self, world: &Arc<World>, potion: &ItemStack) {
        let Some(contents) = potion.get(POTION_CONTENTS) else {
            return;
        };
        let effects = potion_effects(contents);
        if effects.is_empty() {
            return;
        }

        let position = self.position();
        for entity in self.entities_in_splash(world) {
            let Some(living) = entity.as_living_entity() else {
                continue;
            };

            let distance_sqr = entity.position().distance_squared(position);
            if distance_sqr >= SPLASH_RANGE_SQR {
                continue;
            }

            // Vanilla parity: strength falls linearly with distance, reaching
            // zero at the edge of the splash.
            let scale = 1.0 - distance_sqr.sqrt() / SPLASH_RANGE;

            for (effect, duration, amplifier) in &effects {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a scaled tick duration is small and vanilla rounds it"
                )]
                let scaled = scale.mul_add(f64::from(*duration), 0.5) as i32;
                if scaled < MINIMUM_EFFECT_TICKS {
                    continue;
                }
                living.add_mob_effect(MobEffectInstance::with_duration(effect, scaled, *amplifier));
            }
        }
    }
}

impl Entity for SplashPotionEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn tick(&self) {
        self.throwable_projectile_tick();
    }

    fn get_default_gravity(&self) -> f64 {
        // Vanilla parity: `AbstractThrownPotion.getDefaultGravity`. Heavier than
        // a snowball, which is why a splash potion arcs so sharply.
        0.05
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
        self.save_throwable_item(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        self.load_throwable_item(nbt);
    }
}

impl Projectile for SplashPotionEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);

        let Some(world) = self.level() else {
            return;
        };
        if self.is_removed() {
            return;
        }

        let potion = self.get_item();
        let is_water = potion
            .get(POTION_CONTENTS)
            .is_some_and(|contents| contents.is(&vanilla_potions::WATER));

        if is_water {
            self.splash_as_water(&world);
        } else if self.lingers() {
            self.leave_cloud(&world, &potion);
        } else {
            self.splash_as_potion(&world, &potion);
        }

        // TODO: a water bottle also douses fire, candles and campfires on the
        // block it lands against; block dousing is not wired.

        // Vanilla picks a different particle for an instant potion so the burst
        // reads as a hit rather than a lingering effect.
        let has_instant = potion.get(POTION_CONTENTS).is_some_and(|contents| {
            contents
                .potion()
                .is_some_and(|potion| potion.value().effects.iter().any(|e| e.duration <= 1))
        });
        let event = if has_instant {
            PARTICLES_INSTANT_POTION_SPLASH
        } else {
            PARTICLES_SPELL_POTION_SPLASH
        };
        world.level_event(event, self.block_position(), 0, None);

        self.set_removed(RemovalReason::Discarded);
    }
}

impl ThrowableProjectile for SplashPotionEntity {}

impl ThrowableItemProjectile for SplashPotionEntity {
    fn get_default_item(&self) -> ItemRef {
        &vanilla_items::SPLASH_POTION
    }

    fn set_item(&self, item: ItemStack) {
        self.entity_data
            .lock()
            .throwable_item_projectile
            .item_stack
            .set(item);
    }

    fn get_item(&self) -> ItemStack {
        self.entity_data
            .lock()
            .throwable_item_projectile
            .item_stack
            .get()
            .clone()
    }
}
