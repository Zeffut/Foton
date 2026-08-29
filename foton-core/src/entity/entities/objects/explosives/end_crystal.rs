//! End crystal.
//!
//! Vanilla parity: `EndCrystal`. The thing on top of each obsidian pillar that
//! heals the dragon, and the thing four of which are placed to bring it back.
//! It has no health: any hit at all destroys it, and destroying it sets off a
//! six-block blast, which is what makes clearing the pillars a chain reaction.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_damage_type_tags::DamageTypeTag;
use foton_registry::vanilla_damage_types;
use foton_registry::vanilla_entity_data::EndCrystalEntityData;
use foton_registry::vanilla_game_rules::BLOCK_EXPLOSION_DROP_DECAY;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, locks::SyncMutex};
use foton_utils::{Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};

use crate::behavior::blocks::FireBlock;
use crate::entity::damage::DamageSource;
use crate::entity::entities::EnderDragon;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntitySyncedData, RemovalReason};
use crate::world::explosion::ExplosionSpec;
use crate::world::{LevelAccessor as _, LevelReader as _, World};

/// How far the blast a broken crystal leaves reaches.
///
/// Vanilla parity: the `6.0F` of `EndCrystal.hurtServer`.
const EXPLOSION_RADIUS: f32 = 6.0;

/// End Crystal entity state needed by worldgen and persistence.
#[entity_behavior(class = "EndCrystal")]
pub struct EndCrystalEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<EndCrystalEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `EndCrystalEntity`.
unsafe impl DowncastType for EndCrystalEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/end_crystal");
}

impl EndCrystalEntity {
    /// Creates a new End Crystal entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(EndCrystalEntityData::new()),
        }
    }

    /// Creates an End Crystal entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(EndCrystalEntityData::new()),
        }
    }

    /// Sets the optional beam target.
    pub fn set_beam_target(&self, target: Option<BlockPos>) {
        self.entity_data.lock().beam_target.set(target);
    }

    /// Returns the optional beam target.
    #[must_use]
    pub fn beam_target(&self) -> Option<BlockPos> {
        *self.entity_data.lock().beam_target.get()
    }

    /// Sets whether the crystal renders its bedrock base.
    pub fn set_show_bottom(&self, show_bottom: bool) {
        self.entity_data.lock().show_bottom.set(show_bottom);
    }

    /// Returns whether the crystal renders its bedrock base.
    #[must_use]
    pub fn shows_bottom(&self) -> bool {
        *self.entity_data.lock().show_bottom.get()
    }

    /// Sets position and rotation, matching vanilla `Entity.snapTo`.
    ///
    /// # Panics
    ///
    /// Panics if the active world entity manager rejects the snap position. This is an invariant
    /// failure for loaded end crystals.
    pub fn snap_to(&self, position: DVec3, yaw: f32, pitch: f32) {
        if let Err(error) = self.base.try_set_position(position) {
            panic!(
                "failed to commit end crystal {} snap position: {error}",
                self.base.id()
            );
        }
        self.base.set_rotation((yaw, pitch));
        self.set_old_position_to_current();
    }

    const fn nbt_bool(value: bool) -> i8 {
        if value { 1 } else { 0 }
    }

    /// Tells the fight one of its crystals is gone.
    ///
    /// Vanilla parity: `EndCrystal.onDestroyedBy`.
    fn on_destroyed_by(&self, world: &Arc<World>, source: &DamageSource) {
        if let Some(fight) = world.dragon_fight() {
            fight.on_crystal_destroyed(world, self, source);
        }
    }
}

impl Entity for EndCrystalEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `EndCrystal.tick`. The only server work in it is
    /// relighting the fire a crystal of a dragon fight sits in, which is what
    /// makes the pillar crystals burn again after a blast has put them out.
    ///
    /// **Gap**: `applyEffectsFromBlocks` and `handlePortal` are not carried.
    fn tick(&self) {
        let Some(world) = self.level() else {
            return;
        };
        if world.dragon_fight().is_none() {
            return;
        }

        let pos = self.block_position();
        if !world.get_block_state(pos).is_air() {
            return;
        }
        world.set_block_state(
            pos,
            FireBlock::get_state(world.as_ref(), pos),
            UpdateFlags::UPDATE_ALL,
        );
    }

    fn is_pickable(&self) -> bool {
        true
    }

    /// Vanilla parity: `EndCrystal.hurtServer`. A crystal has one point of
    /// nothing: any hit at all removes it, and unless the hit was itself an
    /// explosion it takes a six-block blast with it. That chain -- one arrow,
    /// four crystals, four blasts -- is how the pillars come down.
    ///
    /// Breaking one then calls `onDestroyedBy`, which is the only route from a
    /// crystal to a dragon: the fight aborts a respawn ritual, recounts the
    /// crystals, and hands the news to
    /// [`EnderDragon::on_crystal_destroyed`](crate::entity::entities::EnderDragon::on_crystal_destroyed).
    fn hurt(&self, world: &World, source: &DamageSource, _amount: f32) -> bool {
        if self.is_invulnerable_to_base(source) {
            return false;
        }

        // Vanilla parity: `source.getEntity() instanceof EnderDragon`. A dragon
        // flying through its own crystals must not break them.
        let dealt_by_dragon = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
            .is_some_and(|causing| causing.downcast_ref::<EnderDragon>().is_some());
        if dealt_by_dragon {
            return false;
        }

        if self.is_removed() {
            return true;
        }

        self.set_removed(RemovalReason::Killed);

        let Some(world) = self.level() else {
            return true;
        };

        if !source.is(&DamageTypeTag::IS_EXPLOSION) {
            let damage_source = source.causing_entity_id.map(|causing| {
                DamageSource::environment(&vanilla_damage_types::EXPLOSION)
                    .with_direct_entity(self.id())
                    .with_causing_entity(causing)
            });
            world.explode(
                ExplosionSpec::new(
                    Some(self.id()),
                    source.causing_entity_id,
                    damage_source,
                    EXPLOSION_RADIUS,
                    false,
                    world.explosion_destroy_type(&BLOCK_EXPLOSION_DROP_DECAY),
                ),
                self.position(),
            );
        }

        self.on_destroyed_by(&world, source);
        true
    }

    fn blocks_building(&self) -> bool {
        true
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        if let Some(target) = self.beam_target() {
            nbt.insert(
                "beam_target",
                NbtTag::IntArray(vec![target.x(), target.y(), target.z()]),
            );
        }

        nbt.insert("ShowBottom", Self::nbt_bool(self.shows_bottom()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        if let Some(target) = nbt.int_array("beam_target")
            && target.len() == 3
        {
            self.set_beam_target(Some(BlockPos::new(target[0], target[1], target[2])));
        }

        if let Some(show_bottom) = nbt.byte("ShowBottom") {
            self.set_show_bottom(show_bottom != 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use foton_registry::{init_vanilla_registry, vanilla_damage_types, vanilla_entities};
    use foton_utils::ChunkPos;

    use crate::behavior::init_behaviors;
    use crate::entity::entities::EnderDragon;
    use crate::entity::{SharedEntity, init_entities, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    #[test]
    fn end_crystal_does_not_duplicate_shared_invulnerable_state() {
        let crystal = EndCrystalEntity::new(
            &vanilla_entities::END_CRYSTAL,
            1,
            DVec3::new(1.5, 2.5, 3.5),
            Weak::new(),
        );
        crystal.set_invulnerable(true);

        let mut nbt = NbtCompound::new();
        crystal.save_additional(&mut nbt);

        assert_eq!(nbt.byte("Invulnerable"), None);
    }

    #[test]
    fn end_crystal_is_pickable_like_vanilla() {
        let crystal = EndCrystalEntity::new(
            &vanilla_entities::END_CRYSTAL,
            1,
            DVec3::new(1.5, 2.5, 3.5),
            Weak::new(),
        );

        assert!(crystal.is_pickable());
    }

    /// A crystal used to be indestructible: with no `hurt` override the trait
    /// default refused every hit outright, because a crystal is not a living
    /// entity. Clearing the pillars is the first half of the dragon fight, so
    /// this is the assertion that says the fight is winnable at all.
    #[test]
    fn any_hit_at_all_destroys_a_crystal() {
        let crystal = EndCrystalEntity::new(
            &vanilla_entities::END_CRYSTAL,
            1,
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        );
        let world = fresh_test_world("end_crystal_is_destructible");

        let landed = crystal.hurt(
            world.as_ref(),
            &DamageSource::environment(&vanilla_damage_types::GENERIC),
            0.0,
        );

        assert!(landed, "the crystal refused the hit");
        assert!(crystal.is_removed(), "the crystal survived being hit");
    }

    /// Vanilla parity: the `source.getEntity() instanceof EnderDragon` guard.
    /// A dragon flies through its own crystals constantly; without this it
    /// would break the ones keeping it alive.
    #[test]
    fn a_dragon_cannot_break_the_crystal_it_is_healing_from() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();
        let world = fresh_test_world("end_crystal_ignores_its_dragon");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let dragon = Arc::new(EnderDragon::new(
            &vanilla_entities::ENDER_DRAGON,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        let dragon_id = dragon.id();
        world
            .try_add_entity(dragon as SharedEntity)
            .expect("dragon should spawn");

        let crystal = EndCrystalEntity::new(
            &vanilla_entities::END_CRYSTAL,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        );
        let from_dragon = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
            .with_causing_entity(dragon_id);

        assert!(!crystal.hurt(world.as_ref(), &from_dragon, 10.0));
        assert!(
            !crystal.is_removed(),
            "the dragon broke the crystal it heals from"
        );
    }

    #[test]
    fn end_crystal_blocks_building_like_vanilla() {
        let crystal =
            EndCrystalEntity::new(&vanilla_entities::END_CRYSTAL, 1, DVec3::ZERO, Weak::new());

        assert!(crystal.blocks_building());
    }
}
