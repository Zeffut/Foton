//! The thing an ominous trial spawner drops items out of.
//!
//! Vanilla parity: `net.minecraft.world.entity.OminousItemSpawner`. A tiny
//! invisible entity that hangs above a fight for a few seconds and then lets go
//! of what it is carrying.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::behavior::PushReaction;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_data::OminousItemSpawnerEntityData;
use steel_registry::{level_events, sound_events, vanilla_entities, vanilla_game_events};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, RemovalReason, next_entity_id,
};
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla parity: `OminousItemSpawner.SPAWN_ITEM_DELAY_MIN`.
const SPAWN_ITEM_DELAY_MIN: i64 = 60;
/// Vanilla parity: `OminousItemSpawner.SPAWN_ITEM_DELAY_MAX`.
const SPAWN_ITEM_DELAY_MAX: i64 = 120;
/// Vanilla parity: `OminousItemSpawner.TICKS_BEFORE_ABOUT_TO_SPAWN_SOUND`.
const TICKS_BEFORE_ABOUT_TO_SPAWN_SOUND: i64 = 36;

/// An ominous item spawner.
#[entity_behavior(class = "OminousItemSpawner")]
pub struct OminousItemSpawnerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<OminousItemSpawnerEntityData>,
    spawn_item_after_ticks: AtomicI64,
}

// SAFETY: This key is owned by Steel and uniquely identifies `OminousItemSpawnerEntity`.
unsafe impl DowncastType for OminousItemSpawnerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/ominous_item_spawner");
}

impl OminousItemSpawnerEntity {
    /// Creates one at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(OminousItemSpawnerEntityData::new()),
            spawn_item_after_ticks: AtomicI64::new(0),
        }
    }

    /// Creates one from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(OminousItemSpawnerEntityData::new()),
            spawn_item_after_ticks: AtomicI64::new(0),
        }
    }

    /// Builds one carrying `item`, already positioned.
    ///
    /// Vanilla parity: `OminousItemSpawner.create` followed by the `snapTo` its
    /// only caller does.
    #[must_use]
    pub fn create(world: &Arc<World>, position: DVec3, item: ItemStack) -> Arc<dyn Entity> {
        let spawner = Self::new(
            &vanilla_entities::OMINOUS_ITEM_SPAWNER,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        );
        spawner.spawn_item_after_ticks.store(
            rand::random_range(SPAWN_ITEM_DELAY_MIN..=SPAWN_ITEM_DELAY_MAX),
            Ordering::Relaxed,
        );
        spawner.set_item(item);
        Arc::new(spawner)
    }

    /// Vanilla parity: `OminousItemSpawner.getItem`.
    #[must_use]
    pub fn item(&self) -> ItemStack {
        self.entity_data.lock().item.get().clone()
    }

    fn set_item(&self, item: ItemStack) {
        self.entity_data.lock().item.set(item);
    }

    /// Vanilla parity: `OminousItemSpawner.tickServer`.
    fn tick_server(&self, world: &Arc<World>) {
        let due = self.spawn_item_after_ticks.load(Ordering::Relaxed);
        let ticks = i64::from(self.tick_count());
        if ticks == due - TICKS_BEFORE_ABOUT_TO_SPAWN_SOUND {
            world.play_sound(
                &sound_events::BLOCK_TRIAL_SPAWNER_ABOUT_TO_SPAWN_ITEM,
                SoundSource::Neutral,
                self.block_position(),
                1.0,
                1.0,
                None,
            );
        }
        if ticks < due {
            return;
        }
        self.spawn_item(world);
        self.set_removed(RemovalReason::Killed);
    }

    /// Vanilla parity: `OminousItemSpawner.spawnItem`.
    ///
    /// Vanilla asks the item whether it is a `ProjectileItem` and throws it if
    /// so -- which is what makes an ominous trial rain lingering potions rather
    /// than drop them. Steel has no `ProjectileItem` dispatch and no
    /// `ThrownLingeringPotion` entity, so every item takes vanilla's other
    /// branch and is dropped instead. The named gap is exactly those two.
    fn spawn_item(&self, world: &Arc<World>) {
        let item = self.item();
        if item.is_empty() {
            return;
        }

        let Some(dropped) = world.spawn_item_with_velocity(self.position(), item, DVec3::ZERO)
        else {
            return;
        };

        world.level_event(
            level_events::PARTICLES_TRIAL_SPAWNER_SPAWN_ITEM,
            self.block_position(),
            1,
            None,
        );
        world.game_event_at(
            &vanilla_game_events::ENTITY_PLACE,
            self.position(),
            &GameEventContext::new(Some(dropped.as_ref()), None),
        );
        self.set_item(ItemStack::empty());
    }
}

impl Entity for OminousItemSpawnerEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `OminousItemSpawner`'s constructor sets `noPhysics`.
    fn no_physics(&self) -> bool {
        true
    }

    fn tick(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.tick_server(&world);
    }

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        false
    }

    fn could_accept_passenger(&self) -> bool {
        false
    }

    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }

    fn piston_push_reaction(&self) -> PushReaction {
        PushReaction::Ignore
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let item = self.item();
        if !item.is_empty() {
            nbt.insert("item", item.to_nbt_tag_ref());
        }
        nbt.insert(
            "spawn_item_after_ticks",
            self.spawn_item_after_ticks.load(Ordering::Relaxed),
        );
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let item = nbt
            .compound("item")
            .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
            .unwrap_or_else(ItemStack::empty);
        self.set_item(item);
        self.spawn_item_after_ticks.store(
            nbt.long("spawn_item_after_ticks").unwrap_or(0),
            Ordering::Relaxed,
        );
    }
}
