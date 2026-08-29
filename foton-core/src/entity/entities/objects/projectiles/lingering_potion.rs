//! Thrown lingering potion.
//!
//! Vanilla parity: `ThrownLingeringPotion`. Its sibling
//! [`super::SplashPotionEntity`] is `ThrownSplashPotion`, and the two differ in
//! exactly one method: where a splash doses everything in range once,
//! `onHitAsPotion` here leaves an [`AreaEffectCloudEntity`] behind and doses
//! nobody directly.
//!
//! The rest -- the 0.05 gravity, the water-bottle dousing, the level event
//! chosen by whether the potion is instant -- is `AbstractThrownPotion`, shared
//! with the splash potion.
//!
//! Which of the two entities is thrown is decided by the *item*, in
//! `LingeringPotionItem`, exactly as in vanilla. It is not decided by the
//! bottle the entity carries: a lingering bottle put into a splash-potion
//! entity by a command still splashes, because vanilla's `ThrownSplashPotion`
//! has no lingering branch to take.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::data_components::vanilla_components::POTION_CONTENTS;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::items::ItemRef;
use foton_registry::level_events::{
    PARTICLES_INSTANT_POTION_SPLASH, PARTICLES_SPELL_POTION_SPLASH,
};
use foton_registry::vanilla_entity_data::LingeringPotionEntityData;
use foton_registry::{vanilla_damage_types, vanilla_entities, vanilla_items, vanilla_potions};
use foton_utils::locks::SyncMutex;
use foton_utils::{Downcast as _, DowncastType, DowncastTypeKey, WorldAabb};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::potion_effects;
use crate::entity::damage::DamageSource;
use crate::entity::entities::{AreaEffectCloudEntity, AxolotlEntity};
use crate::entity::next_entity_id;
use crate::entity::projectile::{
    Projectile, ProjectileBase, ProjectileHit, ThrowableItemProjectile, ThrowableProjectile,
};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, RemovalReason, SharedEntity,
};
use crate::world::World;

/// How far a splash reaches.
///
/// Vanilla parity: `AbstractThrownPotion.SPLASH_RANGE`. A lingering potion does
/// not dose at range, but it still uses this box to find the water-sensitive
/// mobs a water bottle burns.
const SPLASH_RANGE: f64 = 4.0;

/// Squared splash range, as vanilla compares it.
const SPLASH_RANGE_SQR: f64 = SPLASH_RANGE * SPLASH_RANGE;

/// How far up and down the splash box reaches.
///
/// Vanilla parity: the `inflate(4.0, 2.0, 4.0)` of `onHitAsWater`.
const SPLASH_HEIGHT: f64 = 2.0;

/// Damage a water bottle does to something that hates water.
///
/// Vanilla parity: the `1.0F` of `onHitAsWater`.
const WATER_DAMAGE: f32 = 1.0;

/// A thrown lingering potion.
#[entity_behavior(class = "ThrownLingeringPotion")]
pub struct LingeringPotionEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<LingeringPotionEntityData>,
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Foton and uniquely identifies
// `LingeringPotionEntity`.
unsafe impl DowncastType for LingeringPotionEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/lingering_potion");
}

impl LingeringPotionEntity {
    /// Creates a thrown lingering potion with no owner.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(LingeringPotionEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a thrown lingering potion from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(LingeringPotionEntityData::new()),
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
    /// Vanilla parity: `AbstractThrownPotion.onHitAsWater`, shared with the
    /// splash potion. A lingering water bottle leaves no cloud at all, because
    /// water has no effects and `onHitAsPotion` is never reached.
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
        // distance test of its own -- an axolotl anywhere in the splash box gets
        // its air back, even one further out than the dousing reaches.
        for entity in self.entities_in_splash(world) {
            if let Some(axolotl) = entity.downcast_ref::<AxolotlEntity>() {
                axolotl.rehydrate();
            }
        }
    }

    /// Leaves a lingering cloud where the bottle broke.
    ///
    /// Vanilla parity: `ThrownLingeringPotion.onHitAsPotion`.
    ///
    /// Vanilla also calls `cloud.setOwner(owner)`. Foton's
    /// [`AreaEffectCloudEntity`] has no owner field, so a cloud's damage is
    /// credited to nobody -- the same gap the dragon fireball's cloud already
    /// documents.
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
}

impl Entity for LingeringPotionEntity {
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
        // Vanilla parity: `AbstractThrownPotion.getDefaultGravity`.
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

impl Projectile for LingeringPotionEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `AbstractThrownPotion.onHit` with
    /// `ThrownLingeringPotion.onHitAsPotion`.
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
        } else {
            self.leave_cloud(&world, &potion);
        }

        // TODO: a water bottle also douses fire, candles and campfires on the
        // block it lands against; block dousing is not wired.

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

impl ThrowableProjectile for LingeringPotionEntity {}

impl ThrowableItemProjectile for LingeringPotionEntity {
    fn get_default_item(&self) -> ItemRef {
        &vanilla_items::LINGERING_POTION
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

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_potions};
    use foton_utils::{BlockPos, ChunkPos, Direction, Identifier};

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::entities::{PigEntity, SplashPotionEntity};
    use crate::entity::next_entity_id;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use crate::world::ClipHitResult;

    fn block_hit(at: DVec3) -> ProjectileHit {
        ProjectileHit::Block {
            location: at,
            hit: ClipHitResult {
                location: at,
                direction: Direction::Up,
                block_pos: BlockPos::new(8, 63, 8),
                miss: false,
                inside: false,
                world_border_hit: false,
            },
        }
    }

    fn swiftness_bottle(item: ItemRef) -> ItemStack {
        let mut stack = ItemStack::new(item);
        stack.set_potion(&Identifier::vanilla_static("swiftness"));
        stack
    }

    fn clouds_around(world: &Arc<World>, at: DVec3) -> usize {
        world
            .get_entities_in_aabb(&WorldAabb::new(
                at.x - 8.0,
                at.y - 8.0,
                at.z - 8.0,
                at.x + 8.0,
                at.y + 8.0,
                at.z + 8.0,
            ))
            .into_iter()
            .filter(|entity| entity.entity_type() == &vanilla_entities::AREA_EFFECT_CLOUD)
            .count()
    }

    /// The one method that separates this class from `ThrownSplashPotion`.
    #[test]
    fn a_lingering_potion_leaves_a_cloud_where_it_lands() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_lingering_potion_cloud");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let potion = LingeringPotionEntity::new(
            &vanilla_entities::LINGERING_POTION,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        );
        potion.set_item(swiftness_bottle(&vanilla_items::LINGERING_POTION));

        let at = potion.position();
        potion.on_hit(&block_hit(at));

        assert_eq!(potion.removal_reason(), Some(RemovalReason::Discarded));
        assert_eq!(clouds_around(&world, at), 1);
    }

    /// The entity decides, not the bottle. Foton used to read the carried item
    /// and linger from inside the splash entity, which meant a lingering bottle
    /// handed to a splash potion left a cloud -- something vanilla's
    /// `ThrownSplashPotion` has no branch for.
    #[test]
    fn a_splash_potion_holding_a_lingering_bottle_leaves_no_cloud() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_splash_potion_never_lingers");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let potion = SplashPotionEntity::new(
            &vanilla_entities::SPLASH_POTION,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        );
        potion.set_item(swiftness_bottle(&vanilla_items::LINGERING_POTION));

        let at = potion.position();
        potion.on_hit(&block_hit(at));

        assert_eq!(potion.removal_reason(), Some(RemovalReason::Discarded));
        assert_eq!(clouds_around(&world, at), 0);
    }

    /// A lingering water bottle takes `onHitAsWater` instead, which puts out
    /// what is burning nearby. Asserting the missing cloud alone would prove
    /// nothing: `leave_cloud` also bails on a bottle with no effects, so it
    /// stays green even when water is routed down the cloud branch.
    #[test]
    fn a_lingering_water_bottle_douses_instead_of_leaving_a_cloud() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_lingering_water_bottle");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let burning: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(9.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        burning.set_remaining_fire_ticks(100);
        world
            .try_add_entity(Arc::clone(&burning))
            .unwrap_or_else(|error| panic!("test pig should enter the world: {error}"));
        assert!(burning.is_on_fire(), "the pig has to start alight");

        let potion = LingeringPotionEntity::new(
            &vanilla_entities::LINGERING_POTION,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        );
        let mut bottle = ItemStack::new(&vanilla_items::LINGERING_POTION);
        bottle.set_potion(&vanilla_potions::WATER.key);
        potion.set_item(bottle);

        let at = potion.position();
        potion.on_hit(&block_hit(at));

        assert!(!burning.is_on_fire(), "a water bottle puts the pig out");
        assert_eq!(clouds_around(&world, at), 0);
    }
}
