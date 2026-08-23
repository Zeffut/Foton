//! Interaction entity.
//!
//! Vanilla parity: `Interaction`. An invisible box of a chosen width and
//! height that cannot be damaged and does nothing on its own. Its whole
//! purpose is bookkeeping: it remembers who last hit it and who last used it,
//! together with the game tick it happened on, and a data pack reads that back
//! through `attack`/`interaction` NBT or the `Attackable`/`Targeting` selectors.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_registry::entity_data::EntityPose;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::vanilla_entity_data::InteractionEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{DowncastType, DowncastTypeKey, UuidExt};
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntitySyncedData, SharedEntity};
use crate::player::Player;
use crate::world::World;

/// Default box width in blocks.
///
/// Vanilla parity: `Interaction.DEFAULT_WIDTH`.
pub const DEFAULT_WIDTH: f32 = 1.0;

/// Default box height in blocks.
///
/// Vanilla parity: `Interaction.DEFAULT_HEIGHT`.
pub const DEFAULT_HEIGHT: f32 = 1.0;

/// Vanilla's eye-height ratio for a scalable hitbox.
///
/// Vanilla parity: `EntityDimensions.defaultEyeHeight`, used because
/// `Interaction.getDimensions` builds its box with `EntityDimensions.scalable`.
const DEFAULT_EYE_HEIGHT_RATIO: f32 = 0.85;

/// One recorded player action against an interaction entity.
///
/// Vanilla parity: `Interaction.PlayerAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerAction {
    /// UUID of the player who acted.
    pub player: Uuid,
    /// Game tick the action happened on.
    pub timestamp: i64,
}

impl PlayerAction {
    /// Writes this action as vanilla's `{player, timestamp}` compound.
    ///
    /// Vanilla parity: `Interaction.PlayerAction.CODEC`, which stores the UUID
    /// through `UUIDUtil.CODEC`, that is as a four-int array.
    fn to_nbt(self) -> NbtCompound {
        let mut compound = NbtCompound::new();
        compound.insert(
            "player",
            NbtTag::IntArray(self.player.to_int_array().to_vec()),
        );
        compound.insert("timestamp", self.timestamp);
        compound
    }

    /// Reads vanilla's `{player, timestamp}` compound.
    ///
    /// Vanilla parity: `Interaction.PlayerAction.CODEC`. Both fields are
    /// required, so a partial compound decodes to nothing rather than to a
    /// half-filled action.
    fn from_nbt(compound: &BorrowedNbtCompoundView<'_, '_>) -> Option<Self> {
        let player = Uuid::from_int_array(&compound.int_array("player")?)?;
        let timestamp = compound.long("timestamp")?;
        Some(Self { player, timestamp })
    }
}

/// The last-action bookkeeping that is the point of the entity.
#[derive(Debug, Default)]
struct InteractionState {
    /// Vanilla parity: `Interaction.attack`.
    attack: Option<PlayerAction>,
    /// Vanilla parity: `Interaction.interaction`.
    interaction: Option<PlayerAction>,
}

/// An interaction entity.
///
/// Vanilla parity: `Interaction`.
#[entity_behavior(class = "Interaction")]
pub struct InteractionEntity {
    /// Common entity fields (id, uuid, position, etc.).
    base: EntityBase,
    /// Vanilla entity type registered for this implementation.
    entity_type: EntityTypeRef,
    /// Synced entity data for network serialization.
    entity_data: SyncMutex<InteractionEntityData>,
    /// Who last hit this box and who last used it.
    state: SyncMutex<InteractionState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `InteractionEntity`.
unsafe impl DowncastType for InteractionEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/interaction");
}

impl InteractionEntity {
    /// Creates a new interaction entity.
    ///
    /// The `id` should be obtained from `next_entity_id()`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        let entity = Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(InteractionEntityData::new()),
            state: SyncMutex::new(InteractionState::default()),
        };
        entity.refresh_dimensions();
        entity
    }

    /// Creates a new interaction entity with a specific UUID.
    ///
    /// The `id` should be obtained from `next_entity_id()`.
    #[must_use]
    pub fn with_uuid(
        entity_type: EntityTypeRef,
        id: i32,
        position: DVec3,
        uuid: Uuid,
        world: Weak<World>,
    ) -> Self {
        let entity = Self {
            base: EntityBase::with_uuid(id, uuid, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(InteractionEntityData::new()),
            state: SyncMutex::new(InteractionState::default()),
        };
        entity.refresh_dimensions();
        entity
    }

    /// Creates an interaction entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        let entity = Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(InteractionEntityData::new()),
            state: SyncMutex::new(InteractionState::default()),
        };
        entity.refresh_dimensions();
        entity
    }

    /// Gets a reference to the entity data for reading/modifying synced state.
    pub const fn entity_data(&self) -> &SyncMutex<InteractionEntityData> {
        &self.entity_data
    }

    /// Returns the box width in blocks.
    ///
    /// Vanilla parity: `Interaction.getWidth`.
    #[must_use]
    pub fn width(&self) -> f32 {
        *self.entity_data.lock().width.get()
    }

    /// Sets the box width in blocks.
    ///
    /// Vanilla parity: `Interaction.setWidth` plus the `refreshDimensions` that
    /// `Interaction.onSyncedDataUpdated` runs when the width changes.
    pub fn set_width(&self, width: f32) {
        self.entity_data.lock().width.set(width);
        self.refresh_dimensions();
    }

    /// Returns the box height in blocks.
    ///
    /// Vanilla parity: `Interaction.getHeight`.
    #[must_use]
    pub fn height(&self) -> f32 {
        *self.entity_data.lock().height.get()
    }

    /// Sets the box height in blocks.
    ///
    /// Vanilla parity: `Interaction.setHeight` plus the `refreshDimensions`
    /// that `Interaction.onSyncedDataUpdated` runs when the height changes.
    pub fn set_height(&self, height: f32) {
        self.entity_data.lock().height.set(height);
        self.refresh_dimensions();
    }

    /// Returns whether the client plays an attack response for this box.
    ///
    /// Vanilla parity: `Interaction.getResponse`.
    #[must_use]
    pub fn response(&self) -> bool {
        *self.entity_data.lock().response.get()
    }

    /// Sets whether the client plays an attack response for this box.
    ///
    /// Vanilla parity: `Interaction.setResponse`.
    pub fn set_response(&self, response: bool) {
        self.entity_data.lock().response.set(response);
    }

    /// Returns who last hit this box and when.
    ///
    /// Vanilla parity: the `Interaction.attack` field behind `getLastAttacker`.
    #[must_use]
    pub fn last_attack(&self) -> Option<PlayerAction> {
        self.state.lock().attack
    }

    /// Returns who last used this box and when.
    ///
    /// Vanilla parity: the `Interaction.interaction` field behind `getTarget`.
    #[must_use]
    pub fn last_interaction(&self) -> Option<PlayerAction> {
        self.state.lock().interaction
    }

    /// Returns the live player who last hit this box, if still online here.
    ///
    /// Vanilla parity: `Interaction.getLastAttacker`, the `Attackable` half.
    #[must_use]
    pub fn last_attacker(&self) -> Option<SharedEntity> {
        self.player_of(self.last_attack()?.player)
    }

    /// Returns the live player who last used this box, if still online here.
    ///
    /// Vanilla parity: `Interaction.getTarget`, the `Targeting` half.
    #[must_use]
    pub fn target(&self) -> Option<SharedEntity> {
        self.player_of(self.last_interaction()?.player)
    }

    /// Resolves a recorded UUID to a live player in this entity's world.
    ///
    /// Vanilla parity: `Level.getPlayerByUUID`, which only ever matches
    /// players, so a non-player sharing the UUID is rejected.
    fn player_of(&self, uuid: Uuid) -> Option<SharedEntity> {
        let world = self.level()?;
        let entity = world.get_entity_by_uuid(&uuid)?;
        entity.as_player().is_some().then_some(entity)
    }

    /// Returns the current game tick, or zero when the world is gone.
    ///
    /// Vanilla parity: the `this.level().getGameTime()` both recorders use.
    fn action_timestamp(&self) -> i64 {
        self.level().map_or(0, |world| world.game_time())
    }
}

impl Entity for InteractionEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Interaction.isPickable`. The box exists to be clicked,
    /// so it has to answer the interaction raycast even though it is invisible.
    fn is_pickable(&self) -> bool {
        true
    }

    /// Vanilla parity: `Interaction.canBeHitByProjectile`. Arrows pass through.
    fn can_be_hit_by_projectile(&self) -> bool {
        false
    }

    /// Vanilla parity: `Interaction.isIgnoringBlockTriggers`.
    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }

    /// Vanilla parity: `Interaction.getDimensions`, built with
    /// `EntityDimensions.scalable(width, height)`.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let height = self.height();
        EntityDimensions::new(self.width(), height, height * DEFAULT_EYE_HEIGHT_RATIO)
    }

    /// Records the hit and swallows the attack.
    ///
    /// Vanilla parity: `Interaction.skipAttackInteraction`. Returning `true`
    /// tells the attack path to stop, which is what keeps the box from taking
    /// or forwarding damage; `response` is what lets a click still register as
    /// a hit on the client. Non-player attackers are ignored outright.
    ///
    /// Deviation: vanilla also fires `CriteriaTriggers.PLAYER_HURT_ENTITY`
    /// here. Steel has no advancement criteria system, so that trigger is not
    /// raised.
    fn skip_attack_interaction(&self, source: &dyn Entity) -> bool {
        let Some(player) = source.as_player() else {
            return false;
        };
        let action = PlayerAction {
            player: player.uuid(),
            timestamp: self.action_timestamp(),
        };
        self.state.lock().attack = Some(action);
        !self.response()
    }

    /// Records the use and consumes the click.
    ///
    /// Vanilla parity: `Interaction.interact`. The server branch always returns
    /// `CONSUME`; the arm swing the player sees comes from the client's own
    /// `response`-driven branch of the same method.
    fn interact(
        &self,
        player: &Player,
        _hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        let action = PlayerAction {
            player: player.uuid(),
            timestamp: self.action_timestamp(),
        };
        self.state.lock().interaction = Some(action);
        InteractionResult::Consume
    }

    /// Vanilla parity: `Interaction.addAdditionalSaveData`.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        let data = self.entity_data.lock();
        nbt.insert("width", *data.width.get());
        nbt.insert("height", *data.height.get());
        nbt.insert("response", i8::from(*data.response.get()));
        drop(data);

        // Vanilla parity: `ValueOutput.storeNullable`, so an action that never
        // happened writes no tag at all rather than an empty compound.
        let state = self.state.lock();
        if let Some(attack) = state.attack {
            nbt.insert("attack", attack.to_nbt());
        }
        if let Some(interaction) = state.interaction {
            nbt.insert("interaction", interaction.to_nbt());
        }
    }

    /// Vanilla parity: `Interaction.readAdditionalSaveData`, ending with the
    /// `setBoundingBox(makeBoundingBox())` that the restored size needs.
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        {
            let mut data = self.entity_data.lock();
            data.width.set(nbt.float("width").unwrap_or(DEFAULT_WIDTH));
            data.height
                .set(nbt.float("height").unwrap_or(DEFAULT_HEIGHT));
            data.response
                .set(nbt.byte("response").is_some_and(|value| value != 0));
        }

        {
            let mut state = self.state.lock();
            state.attack = nbt
                .compound("attack")
                .as_ref()
                .and_then(PlayerAction::from_nbt);
            state.interaction = nbt
                .compound("interaction")
                .as_ref()
                .and_then(PlayerAction::from_nbt);
        }

        self.refresh_dimensions();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use simdnbt::borrow::read_compound;
    use steel_registry::{init_vanilla_registry, vanilla_entities};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::{EntityBaseSaveData, EntityFireFreezeState, next_entity_id};
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    fn interaction_test_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn reload(entity: &InteractionEntity) -> InteractionEntity {
        let mut saved = NbtCompound::new();
        entity.save_additional(&mut saved);
        let mut bytes = Vec::new();
        saved.write(&mut bytes);
        let Ok(borrowed) = read_compound(&mut Cursor::new(bytes.as_slice())) else {
            panic!("saved interaction NBT should reborrow");
        };
        let loaded = InteractionEntity::from_saved(
            &vanilla_entities::INTERACTION,
            EntityBaseLoad {
                id: 22,
                position: DVec3::ZERO,
                uuid: Uuid::nil(),
                velocity: DVec3::ZERO,
                rotation: (0.0, 0.0),
                fall_distance: 0.0,
                fire_freeze: EntityFireFreezeState::new(),
                on_ground: false,
                save_data: EntityBaseSaveData::new(),
                world: Weak::new(),
            },
        );
        loaded.load_additional((&borrowed).into());
        loaded
    }

    #[test]
    fn a_saved_interaction_comes_back_with_its_size_response_and_both_recorded_actions() {
        init_vanilla_registry();
        let entity = InteractionEntity::new(
            &vanilla_entities::INTERACTION,
            21,
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        );
        entity.set_width(2.5);
        entity.set_height(4.0);
        entity.set_response(true);
        let attacker = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
        let user = Uuid::from_u128(0x0fed_cba9_8765_4321_0fed_cba9_8765_4321);
        {
            let mut state = entity.state.lock();
            state.attack = Some(PlayerAction {
                player: attacker,
                timestamp: 1234,
            });
            state.interaction = Some(PlayerAction {
                player: user,
                timestamp: 5678,
            });
        }

        let loaded = reload(&entity);

        assert_eq!(loaded.width().to_bits(), 2.5_f32.to_bits());
        assert_eq!(loaded.height().to_bits(), 4.0_f32.to_bits());
        assert!(loaded.response());
        assert_eq!(
            loaded.last_attack(),
            Some(PlayerAction {
                player: attacker,
                timestamp: 1234,
            })
        );
        assert_eq!(
            loaded.last_interaction(),
            Some(PlayerAction {
                player: user,
                timestamp: 5678,
            })
        );
    }

    #[test]
    fn an_interaction_that_was_never_touched_saves_no_action_tags() {
        init_vanilla_registry();
        let entity =
            InteractionEntity::new(&vanilla_entities::INTERACTION, 21, DVec3::ZERO, Weak::new());

        let mut saved = NbtCompound::new();
        entity.save_additional(&mut saved);

        assert!(saved.get("attack").is_none());
        assert!(saved.get("interaction").is_none());

        let loaded = reload(&entity);
        assert_eq!(loaded.width().to_bits(), DEFAULT_WIDTH.to_bits());
        assert_eq!(loaded.height().to_bits(), DEFAULT_HEIGHT.to_bits());
        assert!(!loaded.response());
        assert_eq!(loaded.last_attack(), None);
        assert_eq!(loaded.last_interaction(), None);
    }

    #[test]
    fn resizing_an_interaction_resizes_the_box_players_can_click() {
        init_vanilla_registry();
        let entity = InteractionEntity::new(
            &vanilla_entities::INTERACTION,
            21,
            DVec3::new(0.0, 64.0, 0.0),
            Weak::new(),
        );

        entity.set_width(3.0);
        entity.set_height(2.0);

        let bounding_box = entity.bounding_box();
        assert_eq!(bounding_box.width().to_bits(), 3.0_f64.to_bits());
        assert_eq!(bounding_box.height().to_bits(), 2.0_f64.to_bits());
    }

    #[test]
    fn hitting_and_using_an_interaction_records_the_player_and_the_game_tick() {
        let world = interaction_test_world("interaction_records_actions");
        let player =
            TestPlayerBuilder::new(Arc::clone(&world), "Clicker", next_entity_id()).build();
        let entity = InteractionEntity::new(
            &vanilla_entities::INTERACTION,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        );

        assert!(
            entity.skip_attack_interaction(player.as_ref()),
            "an interaction without a response swallows the attack"
        );
        let result = entity.interact(&player, InteractionHand::MainHand, DVec3::ZERO);

        assert_eq!(result, InteractionResult::Consume);
        let game_time = world.game_time();
        assert_eq!(
            entity.last_attack(),
            Some(PlayerAction {
                player: player.uuid(),
                timestamp: game_time,
            })
        );
        assert_eq!(
            entity.last_interaction(),
            Some(PlayerAction {
                player: player.uuid(),
                timestamp: game_time,
            })
        );
    }

    #[test]
    fn an_interaction_with_a_response_lets_the_attack_through_after_recording_it() {
        let world = interaction_test_world("interaction_response_passes_attack");
        let player =
            TestPlayerBuilder::new(Arc::clone(&world), "Responder", next_entity_id()).build();
        let entity = InteractionEntity::new(
            &vanilla_entities::INTERACTION,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        );
        entity.set_response(true);

        assert!(!entity.skip_attack_interaction(player.as_ref()));
        assert_eq!(
            entity.last_attack().map(|action| action.player),
            Some(player.uuid())
        );
    }
}
