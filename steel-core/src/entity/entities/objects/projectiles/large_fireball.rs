//! Ghast fireball.
//!
//! Vanilla parity: `LargeFireball`, on top of `Fireball` and
//! `AbstractHurtingProjectile`. The explosion it leaves is the whole point: the
//! power is stored on the entity rather than fixed by the class, which is what
//! lets a ghast throw a 1-power ball while `/summon` can throw one that levels a
//! hill. Whether it breaks anything at all is the `mobGriefing` game rule's call,
//! and the same rule decides whether it leaves fires behind.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::ToNbtTag;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_data::FireballEntityData;
use steel_registry::vanilla_game_rules::MOB_GRIEFING;
use steel_registry::{vanilla_damage_types, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, HurtingProjectile, HurtingProjectileBase,
    Projectile, ProjectileBase, ProjectileHit, RemovalReason, SharedEntity,
};
use crate::world::World;
use crate::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};

/// Blast power a fireball carries unless something says otherwise.
///
/// Vanilla parity: `LargeFireball.DEFAULT_EXPLOSION_POWER`.
const DEFAULT_EXPLOSION_POWER: i32 = 1;

/// Damage the fireball does to whatever it strikes head on.
///
/// Vanilla parity: the `6.0F` of `LargeFireball.onHitEntity`. This is on top of
/// the blast, so a direct hit hurts twice.
const DIRECT_HIT_DAMAGE: f32 = 6.0;

/// A ghast's fireball.
#[entity_behavior(class = "LargeFireball")]
pub struct LargeFireballEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<FireballEntityData>,
    projectile_base: ProjectileBase,
    hurting_projectile_base: HurtingProjectileBase,
    /// Radius of the blast it leaves (vanilla `LargeFireball.explosionPower`).
    explosion_power: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `LargeFireballEntity`.
unsafe impl DowncastType for LargeFireballEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/large_fireball");
}

impl LargeFireballEntity {
    /// Creates a fireball with vanilla's default blast power.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(FireballEntityData::new()),
            projectile_base: ProjectileBase::new(),
            hurting_projectile_base: HurtingProjectileBase::new(),
            explosion_power: SyncMutex::new(DEFAULT_EXPLOSION_POWER),
        }
    }

    /// Creates a fireball from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(FireballEntityData::new()),
            projectile_base: ProjectileBase::new(),
            hurting_projectile_base: HurtingProjectileBase::new(),
            explosion_power: SyncMutex::new(DEFAULT_EXPLOSION_POWER),
        }
    }

    /// Returns the radius of the blast this fireball leaves.
    #[must_use]
    pub fn explosion_power(&self) -> i32 {
        *self.explosion_power.lock()
    }

    /// Sets the radius of the blast this fireball leaves.
    ///
    /// Vanilla parity: the `explosionPower` argument of the `LargeFireball`
    /// constructor a ghast uses.
    pub fn set_explosion_power(&self, power: i32) {
        *self.explosion_power.lock() = power;
    }

    /// Returns the item the client draws in place of the fireball.
    ///
    /// Vanilla parity: `Fireball.getItem`.
    #[must_use]
    pub fn item(&self) -> ItemStack {
        self.entity_data.lock().item_stack.get().clone()
    }

    /// Sets the item the client draws in place of the fireball.
    ///
    /// Vanilla parity: `Fireball.setItem`, which falls back to the fire charge
    /// for an empty stack and never carries a count above one.
    pub fn set_item(&self, item: ItemStack) {
        let item = if item.is_empty() {
            ItemStack::new(&vanilla_items::FIRE_CHARGE)
        } else {
            item.copy_with_count(1)
        };
        self.entity_data.lock().item_stack.set(item);
    }

    /// Detonates where the fireball landed.
    ///
    /// Vanilla parity: the explosion of `LargeFireball.onHit`, which passes
    /// `ExplosionInteraction.MOB` and the `mobGriefing` rule as the fire flag.
    ///
    /// Deviations from vanilla, both from Steel's shared explosion API:
    /// `ExplosionInteraction.MOB` resolves to `DESTROY_WITH_DECAY`, which thins
    /// out the drops, and Steel has only a plain `Destroy` that drops
    /// everything; and the blast is credited to the fireball rather than to the
    /// ghast, because `World::hurt_entities_from_explosion` overwrites the
    /// causing entity with the source entity id.
    fn explode(&self, world: &Arc<World>) {
        let mob_griefing = world.get_game_rule(&MOB_GRIEFING);
        let interaction = if mob_griefing {
            ExplosionBlockInteraction::Destroy
        } else {
            ExplosionBlockInteraction::Keep
        };
        world.explode(
            ExplosionSpec::new(
                Some(self.id()),
                self.get_owner().map(|owner| owner.id()),
                None,
                self.explosion_power() as f32,
                mob_griefing,
                interaction,
            ),
            self.position(),
        );
    }
}

impl Entity for LargeFireballEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        self.hurting_projectile_tick();
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn hurt(&self, _world: &World, _source: &DamageSource, _amount: f32) -> bool {
        // Vanilla parity: `AbstractHurtingProjectile.hurtServer` returns false,
        // so a fireball cannot be shot down -- only deflected.
        false
    }

    fn restore_owner_reference(&self, owner: &SharedEntity) {
        self.cache_owner_entity(owner);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
        self.save_hurting_projectile(nbt);
        let item = self.item();
        if !item.is_empty() {
            nbt.insert("Item", item.to_nbt_tag());
        }
        nbt.insert("ExplosionPower", self.explosion_power() as i8);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        self.load_hurting_projectile(nbt);
        if let Some(item) = nbt
            .compound("Item")
            .and_then(|tag| ItemStack::from_borrowed_compound(&tag))
        {
            self.set_item(item);
        }
        self.set_explosion_power(i32::from(
            nbt.byte("ExplosionPower")
                .unwrap_or(DEFAULT_EXPLOSION_POWER as i8),
        ));
    }
}

impl Projectile for LargeFireballEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn on_deflection(&self, by_attack: bool) {
        self.hurting_projectile_on_deflection(by_attack);
    }

    /// Vanilla parity: `LargeFireball.onHitEntity`.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let Some(world) = self.level() else {
            return;
        };

        // Vanilla parity: `DamageSources.fireball` swaps to the unattributed
        // type when the shooter is gone, so the death message still reads.
        let source = match self.get_owner() {
            Some(owner) => DamageSource::environment(&vanilla_damage_types::FIREBALL)
                .with_direct_entity(self.id())
                .with_causing_entity(owner.id()),
            None => DamageSource::environment(&vanilla_damage_types::UNATTRIBUTED_FIREBALL)
                .with_direct_entity(self.id())
                .with_causing_entity(self.id()),
        };
        entity.hurt(&world, &source, DIRECT_HIT_DAMAGE);

        // TODO: vanilla also runs `EnchantmentHelper.doPostAttackEffects` here;
        // Steel has no post-attack enchantment dispatch for projectiles yet.
    }

    /// Vanilla parity: `LargeFireball.onHit`.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);

        let Some(world) = self.level() else {
            return;
        };
        if self.is_removed() {
            return;
        }

        self.explode(&world);
        self.set_removed(RemovalReason::Discarded);
    }
}

impl HurtingProjectile for LargeFireballEntity {
    fn hurting_projectile_base(&self) -> &HurtingProjectileBase {
        &self.hurting_projectile_base
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use glam::DVec3;
    use steel_registry::item_stack::ItemStack;
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
    use steel_utils::{ChunkPos, Direction};

    use crate::behavior::init_behaviors;
    use crate::entity::{Entity, Projectile, ProjectileHit, RemovalReason, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use crate::world::{ClipHitResult, World};

    use super::{DEFAULT_EXPLOSION_POWER, LargeFireballEntity};

    fn fireball(world: Weak<World>) -> LargeFireballEntity {
        init_vanilla_registry();
        LargeFireballEntity::new(
            &vanilla_entities::FIREBALL,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            world,
        )
    }

    #[test]
    fn a_fireball_starts_with_vanillas_one_block_blast() {
        let fireball = fireball(Weak::new());

        assert_eq!(fireball.explosion_power(), DEFAULT_EXPLOSION_POWER);
    }

    #[test]
    fn the_rendered_item_falls_back_to_a_fire_charge_when_cleared() {
        let fireball = fireball(Weak::new());

        fireball.set_item(ItemStack::empty());

        assert!(fireball.item().is(&vanilla_items::FIRE_CHARGE));
    }

    #[test]
    fn landing_detonates_the_fireball_and_spends_it() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_large_fireball_hit");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let fireball = fireball(Arc::downgrade(&world));

        fireball.on_hit(&ProjectileHit::Block {
            location: fireball.position(),
            hit: ClipHitResult {
                location: fireball.position(),
                direction: Direction::Up,
                block_pos: fireball.block_position(),
                miss: false,
                inside: false,
                world_border_hit: false,
            },
        });

        assert_eq!(fireball.removal_reason(), Some(RemovalReason::Discarded));
    }
}
