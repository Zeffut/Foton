//! Blaze fireball.
//!
//! Vanilla parity: `SmallFireball`, on top of `Fireball` and
//! `AbstractHurtingProjectile`. The small one leaves no crater at all: it burns.
//! Five damage and five seconds alight on whatever it strikes, and a fire on the
//! face of whatever block it lands against -- which is why a blaze can burn a
//! wooden roof down without ever breaking a block.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_entity_data::SmallFireballEntityData;
use foton_registry::vanilla_game_rules::MOB_GRIEFING;
use foton_registry::{vanilla_damage_types, vanilla_items};
use foton_utils::locks::SyncMutex;
use foton_utils::types::UpdateFlags;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::ToNbtTag;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::blocks::FireBlock;
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, HurtingProjectile, HurtingProjectileBase,
    Projectile, ProjectileBase, ProjectileHit, RemovalReason, SharedEntity,
};
use crate::inventory::slot_ranges::CONTENTS_SLOT;
use crate::world::{ClipHitResult, World};

/// Damage a small fireball does on a direct hit.
///
/// Vanilla parity: the `5.0F` of `SmallFireball.onHitEntity`.
const DIRECT_HIT_DAMAGE: f32 = 5.0;

/// How long the target burns for, in ticks.
///
/// Vanilla parity: the `igniteForSeconds(5.0F)` of `SmallFireball.onHitEntity`.
const TARGET_BURN_TICKS: i32 = 100;

/// A blaze's fireball.
#[entity_behavior(class = "SmallFireball")]
pub struct SmallFireballEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<SmallFireballEntityData>,
    projectile_base: ProjectileBase,
    hurting_projectile_base: HurtingProjectileBase,
}

// SAFETY: This key is owned by Foton and uniquely identifies `SmallFireballEntity`.
unsafe impl DowncastType for SmallFireballEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/small_fireball");
}

impl SmallFireballEntity {
    /// Creates a small fireball.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(SmallFireballEntityData::new()),
            projectile_base: ProjectileBase::new(),
            hurting_projectile_base: HurtingProjectileBase::new(),
        }
    }

    /// Creates a small fireball from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(SmallFireballEntityData::new()),
            projectile_base: ProjectileBase::new(),
            hurting_projectile_base: HurtingProjectileBase::new(),
        }
    }

    /// Returns the item the client draws in place of the fireball.
    ///
    /// Vanilla parity: `Fireball.getItem`.
    #[must_use]
    pub fn item(&self) -> ItemStack {
        self.entity_data.lock().fireball.item_stack.get().clone()
    }

    /// Sets the item the client draws in place of the fireball.
    ///
    /// Vanilla parity: `Fireball.setItem`.
    pub fn set_item(&self, item: ItemStack) {
        let item = if item.is_empty() {
            ItemStack::new(&vanilla_items::FIRE_CHARGE)
        } else {
            item.copy_with_count(1)
        };
        self.entity_data.lock().fireball.item_stack.set(item);
    }
}

impl Entity for SmallFireballEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `Fireball.getSlot`, whose one slot is the item the fireball displays.
    fn slot_item(&self, slot: i32) -> Option<ItemStack> {
        if slot == CONTENTS_SLOT {
            return Some(self.item());
        }
        self.entity_slot_item(slot)
    }

    fn tick(&self) {
        self.hurting_projectile_tick();
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn hurt(&self, _world: &World, _source: &DamageSource, _amount: f32) -> bool {
        // Vanilla parity: `AbstractHurtingProjectile.hurtServer` returns false.
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
    }
}

impl Projectile for SmallFireballEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn on_deflection(&self, by_attack: bool) {
        self.hurting_projectile_on_deflection(by_attack);
    }

    /// Vanilla parity: `SmallFireball.onHitEntity`.
    ///
    /// The order matters: vanilla lights the target before rolling the damage
    /// and puts the old fire ticks back if the damage was refused, so a hit that
    /// lands during invulnerability frames does not set anything alight either.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let Some(world) = self.level() else {
            return;
        };

        let fire_ticks_before = entity.remaining_fire_ticks();
        entity.ignite_for_ticks(TARGET_BURN_TICKS);

        let source = match self.get_owner() {
            Some(owner) => DamageSource::environment(&vanilla_damage_types::FIREBALL)
                .with_direct_entity(self.id())
                .with_causing_entity(owner.id()),
            None => DamageSource::environment(&vanilla_damage_types::UNATTRIBUTED_FIREBALL)
                .with_direct_entity(self.id())
                .with_causing_entity(self.id()),
        };
        if !entity.hurt(&world, &source, DIRECT_HIT_DAMAGE) {
            entity.set_remaining_fire_ticks(fire_ticks_before);
        }

        // TODO: vanilla also runs `EnchantmentHelper.doPostAttackEffects` on a
        // successful hit; Foton has no post-attack enchantment dispatch for
        // projectiles yet.
    }

    /// Vanilla parity: `SmallFireball.onHitBlock`.
    ///
    /// A mob's fireball only sets fires when `mobGriefing` is on; one thrown by
    /// a player or a dispenser always does.
    fn on_hit_block(&self, hit: &ClipHitResult) {
        self.projectile_on_hit_block(hit);

        let Some(world) = self.level() else {
            return;
        };
        let shot_by_mob = self.get_owner().is_some_and(|owner| owner.is_mob());
        if shot_by_mob && !world.get_game_rule(&MOB_GRIEFING) {
            return;
        }

        let pos = hit.block_pos.relative(hit.direction);
        if world.get_block_state(pos).is_air() {
            world.set_block(
                pos,
                FireBlock::get_state(world.as_ref(), pos),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }

    /// Vanilla parity: `SmallFireball.onHit`, which just breaks up on contact.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        if self.is_removed() {
            return;
        }
        self.set_removed(RemovalReason::Discarded);
    }
}

impl HurtingProjectile for SmallFireballEntity {
    fn hurting_projectile_base(&self) -> &HurtingProjectileBase {
        &self.hurting_projectile_base
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::blocks::block_state_ext::BlockStateExt as _;
    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
    use foton_utils::types::UpdateFlags;
    use foton_utils::{BlockPos, ChunkPos, Direction};
    use glam::DVec3;

    use crate::behavior::init_behaviors;
    use crate::entity::{Projectile, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use crate::world::ClipHitResult;

    use super::SmallFireballEntity;

    #[test]
    fn a_fireball_that_lands_on_stone_lights_the_air_above_it() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_small_fireball_sets_fire");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let ground = BlockPos::new(8, 64, 8);
        assert!(world.set_block(
            ground,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));

        let fireball = SmallFireballEntity::new(
            &vanilla_entities::SMALL_FIREBALL,
            next_entity_id(),
            DVec3::new(8.5, 65.0, 8.5),
            Arc::downgrade(&world),
        );
        fireball.on_hit_block(&ClipHitResult {
            location: DVec3::new(8.5, 65.0, 8.5),
            direction: Direction::Up,
            block_pos: ground,
            miss: false,
            inside: false,
            world_border_hit: false,
        });

        assert_eq!(
            world.get_block_state(ground.above()).get_block(),
            &vanilla_blocks::FIRE
        );
    }

    #[test]
    fn a_fireball_leaves_an_occupied_space_alone() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_small_fireball_occupied");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let ground = BlockPos::new(8, 64, 8);
        assert!(world.set_block(
            ground,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));
        assert!(world.set_block(
            ground.above(),
            vanilla_blocks::COBBLESTONE.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));

        let fireball = SmallFireballEntity::new(
            &vanilla_entities::SMALL_FIREBALL,
            next_entity_id(),
            DVec3::new(8.5, 65.0, 8.5),
            Arc::downgrade(&world),
        );
        fireball.on_hit_block(&ClipHitResult {
            location: DVec3::new(8.5, 65.0, 8.5),
            direction: Direction::Up,
            block_pos: ground,
            miss: false,
            inside: false,
            world_border_hit: false,
        });

        assert_eq!(
            world.get_block_state(ground.above()).get_block(),
            &vanilla_blocks::COBBLESTONE
        );
    }
}
