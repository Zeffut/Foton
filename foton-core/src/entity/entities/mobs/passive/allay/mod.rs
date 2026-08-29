//! Allay entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.allay.Allay`. An allay is
//! a flying `PathfinderMob` with no fight in it at all. Hand it an item and it
//! remembers you, hunts the ground for more of the same, and brings them back
//! -- to your hand, or to the note block it last heard, which is what turns a
//! pair of them into a sorting machine. Play it a jukebox and it dances; give a
//! dancing one an amethyst shard and there are two.

mod allay_ai;

use std::mem::take;
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::data_components::vanilla_components;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::game_events::GameEventRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_blocks, vanilla_entities,
    vanilla_game_events, vanilla_game_rules,
};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::behavior::utils::{block_closer_to_center_than, throw_item};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::damage::DamageSource;
use crate::entity::inventory_carrier::{self, InventoryCarrier, load_inventory, save_inventory};
use crate::entity::mob::{MoveControlKind, NavigationKind};
use crate::entity::{
    ENTITIES, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, LivingEntitySyncedData, Mob, MobBase, MoveResult,
    PathfinderMob, SharedEntity, next_entity_id,
};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::game_event::vibrations::{
    VIBRATION_DATA_TAG, VibrationListener, VibrationPositionSource, VibrationUser,
};
use crate::world::game_event::{
    DynamicGameEventListener, DynamicListenerAction, GameEventContext, GameEventListener,
    SharedGameEventListener,
};
use crate::world::{LevelReader as _, World};

use foton_registry::vanilla_entity_data::AllayEntityData;
use foton_registry::vanilla_game_event_tags::GameEventTag;
use foton_utils::Identifier;

/// Vanilla parity: `Allay.ITEM_PICKUP_REACH`, which unlike every other mob's
/// reaches a block upward as well as sideways -- an allay hovers.
const ITEM_PICKUP_REACH: DVec3 = DVec3::new(1.0, 1.0, 1.0);
/// Vanilla parity: `Allay.DUPLICATION_COOLDOWN_TICKS`, five minutes.
const DUPLICATION_COOLDOWN_TICKS: i64 = 6000;
/// Vanilla parity: the `getSoundVolume` of `Allay`.
const SOUND_VOLUME: f32 = 0.4;
/// Vanilla parity: the `FlyingMoveControl(this, 20, true)` of the constructor.
const MOVE_CONTROL_MAX_TURN: f32 = 20.0;
/// Vanilla parity: the `setRequiredPathLength(48.0F)` of `createNavigation`.
const REQUIRED_PATH_LENGTH: f32 = 48.0;
/// Vanilla parity: the `heal(1.0F)` an allay gets every half second.
const REGENERATION_PER_HALF_SECOND: f32 = 1.0;
/// Vanilla parity: the `tickCount % 10` of `Allay.aiStep`.
const REGENERATION_INTERVAL: i32 = 10;
/// Vanilla parity: the `tickCount % 20` a dancing allay checks its jukebox on.
const DANCE_CHECK_INTERVAL: i32 = 20;
/// Vanilla parity: the `(byte)18` entity event that shows the duplication
/// hearts -- the same byte a breeding pair sends.
const DUPLICATION_HEARTS_EVENT: EntityStatus = EntityStatus::InLoveHearts;
/// Vanilla parity: the `2.0F` volume of every allay interaction sound.
const INTERACTION_SOUND_VOLUME: f32 = 2.0;
/// Vanilla parity: `Allay.VibrationUser.VIBRATION_EVENT_LISTENER_RANGE`.
const NOTE_BLOCK_LISTENER_RADIUS: i32 = 16;
/// Vanilla parity: the `1024` of `GlobalPos.isCloseEnough` in `canReceiveVibration`.
const LIKED_NOTEBLOCK_RANGE: f64 = 1024.0;

/// An allay.
#[entity_behavior(class = "Allay")]
pub struct AllayEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    brain: Brain,
    /// Vanilla parity: the `SimpleContainer(1)` an allay carries.
    inventory: SyncMutex<SimpleContainer>,
    /// Vanilla parity: `Allay.jukeboxPos`.
    jukebox_pos: SyncMutex<Option<BlockPos>>,
    /// Vanilla parity: `Allay.duplicationCooldown`.
    duplication_cooldown: SyncMutex<i64>,
    entity_data: SyncMutex<AllayEntityData>,
    /// The two listeners vanilla hangs on the allay, kept together so
    /// [`Entity::update_dynamic_game_event_listener`] can move both at once.
    listeners: SyncMutex<Vec<DynamicGameEventListener>>,
    /// Vanilla parity: the `VibrationSystem.Listener` of the pair, kept separately
    /// because the allay has to tick it as well as move it.
    vibration_listener: SyncMutex<Option<Arc<VibrationListener>>>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `AllayEntity`.
unsafe impl DowncastType for AllayEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/allay");
}

impl AllayEntity {
    /// Creates an allay at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an allay from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        {
            // Vanilla parity: `Allay.createNavigation`.
            let mut navigation = mob_base.navigation().lock();
            navigation.set_can_open_doors(false);
            navigation.set_can_float(true);
            navigation
                .set_required_path_length(REQUIRED_PATH_LENGTH, f64::from(REQUIRED_PATH_LENGTH));
        }
        let mut entity_data = AllayEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            brain: allay_ai::make_brain(),
            inventory: SyncMutex::new(SimpleContainer::new(1)),
            jukebox_pos: SyncMutex::new(None),
            duplication_cooldown: SyncMutex::new(0),
            entity_data: SyncMutex::new(entity_data),
            listeners: SyncMutex::new(Vec::new()),
            vibration_listener: SyncMutex::new(None),
        }
    }

    /// Attaches the two game-event listeners an allay carries.
    ///
    /// Vanilla builds them in the constructor from `this`. Foton cannot: the
    /// constructor has no `Arc` to hand out and there is no `Arc<dyn Entity>`
    /// downcast, so each listener holds the world and the entity id instead and
    /// looks the allay back up. The world hands the world over the first time it
    /// asks the allay to register.
    fn ensure_listeners(&self, world: &Arc<World>) {
        let mut listeners = self.listeners.lock();
        if !listeners.is_empty() {
            return;
        }
        let source = ListenerSource {
            entity_id: self.id(),
            world: Arc::downgrade(world),
        };
        let jukebox: SharedGameEventListener = Arc::new(JukeboxListener {
            allay: source.clone(),
        });
        let vibration = Arc::new(VibrationListener::new(Arc::new(AllayVibrationUser {
            allay: source,
        })));
        listeners.push(DynamicGameEventListener::new(jukebox));
        listeners.push(DynamicGameEventListener::new(
            Arc::clone(&vibration) as SharedGameEventListener
        ));
        *self.vibration_listener.lock() = Some(vibration);
    }

    fn vibration_listener(&self) -> Option<Arc<VibrationListener>> {
        self.vibration_listener.lock().clone()
    }

    /// Returns vanilla `Allay.isDancing`.
    #[must_use]
    pub fn is_dancing(&self) -> bool {
        *self.entity_data.lock().dancing.get()
    }

    /// Sets vanilla `Allay.setDancing`.
    ///
    /// A panicking allay refuses to start: vanilla will not let a burning allay
    /// dance while it runs for water.
    pub fn set_dancing(&self, dancing: bool) {
        if dancing && self.is_panicking() {
            return;
        }
        self.entity_data.lock().dancing.set(dancing);
    }

    /// Returns vanilla `Allay.canDuplicate`.
    #[must_use]
    pub fn can_duplicate(&self) -> bool {
        *self.entity_data.lock().can_duplicate.get()
    }

    /// Returns vanilla `Allay.duplicationCooldown`.
    #[must_use]
    pub fn duplication_cooldown(&self) -> i64 {
        *self.duplication_cooldown.lock()
    }

    /// Sets vanilla `Allay.setDuplicationCooldown`, which is also where the
    /// synced "can duplicate" flag comes from.
    pub fn set_duplication_cooldown(&self, duplication_cooldown: i64) {
        *self.duplication_cooldown.lock() = duplication_cooldown;
        self.entity_data
            .lock()
            .can_duplicate
            .set(duplication_cooldown == 0);
    }

    /// Vanilla parity: `Allay.resetDuplicationCooldown`.
    pub fn reset_duplication_cooldown(&self) {
        self.set_duplication_cooldown(DUPLICATION_COOLDOWN_TICKS);
    }

    /// Vanilla parity: `Allay.updateDuplicationCooldown`.
    fn update_duplication_cooldown(&self) {
        let cooldown = self.duplication_cooldown();
        if cooldown > 0 {
            self.set_duplication_cooldown(cooldown - 1);
        }
    }

    /// Returns vanilla `Allay.hasItemInHand`.
    #[must_use]
    pub fn has_item_in_hand(&self) -> bool {
        !self.get_item_in_hand(InteractionHand::MainHand).is_empty()
    }

    /// Vanilla parity: `Allay.isOnPickupCooldown`.
    fn is_on_pickup_cooldown(&self) -> bool {
        self.brain
            .has_memory_value(memory_module_types::ITEM_PICKUP_COOLDOWN_TICKS.id())
    }

    /// Vanilla parity: `Allay.isLikedPlayer`.
    #[must_use]
    pub fn is_liked_player(&self, other: Option<&dyn Entity>) -> bool {
        let Some(other) = other else {
            return false;
        };
        if other.as_player().is_none() {
            return false;
        }
        self.brain
            .get_memory(memory_module_types::LIKED_PLAYER)
            .is_some_and(|liked| liked == other.uuid())
    }

    /// Vanilla parity: `Allay.allayConsidersItemEqual`.
    ///
    /// The potion half matters: two splash potions of different brews are the
    /// same item, and an allay that ignored their contents would fetch the wrong
    /// one.
    #[must_use]
    pub fn considers_item_equal(first: &ItemStack, second: &ItemStack) -> bool {
        ItemStack::is_same_item(first, second)
            && first.get(vanilla_components::POTION_CONTENTS)
                == second.get(vanilla_components::POTION_CONTENTS)
    }

    /// Vanilla parity: `Allay.setJukeboxPlaying`.
    pub fn set_jukebox_playing(&self, jukebox: BlockPos, is_playing: bool) {
        if is_playing {
            if !self.is_dancing() {
                *self.jukebox_pos.lock() = Some(jukebox);
                self.set_dancing(true);
            }
            return;
        }

        let matches = {
            let current = self.jukebox_pos.lock();
            current.is_none_or(|pos| pos == jukebox)
        };
        if matches {
            *self.jukebox_pos.lock() = None;
            self.set_dancing(false);
        }
    }

    /// Vanilla parity: `Allay.shouldStopDancing`.
    fn should_stop_dancing(&self) -> bool {
        let Some(jukebox_pos) = *self.jukebox_pos.lock() else {
            return true;
        };
        let Some(world) = self.level() else {
            return true;
        };
        let radius = f64::from(vanilla_game_events::JUKEBOX_PLAY.notification_radius);
        !block_closer_to_center_than(jukebox_pos, self.position(), radius)
            || world.get_block_state(jukebox_pos).get_block() != &vanilla_blocks::JUKEBOX
    }

    /// Vanilla parity: `Allay.duplicateAllay`.
    fn duplicate_allay(&self, world: &Arc<World>) -> Option<SharedEntity> {
        let allay = Arc::new(Self::new(
            &vanilla_entities::ALLAY,
            next_entity_id(),
            self.position(),
            Arc::downgrade(world),
        ));
        allay.set_old_position_to_current();
        allay.set_persistence_required();
        allay.reset_duplication_cooldown();
        self.reset_duplication_cooldown();

        let shared: SharedEntity = allay;
        world.try_add_entity(Arc::clone(&shared)).ok()?;
        Some(shared)
    }

    /// Vanilla parity: the amethyst-shard branch of `Allay.mobInteract`.
    fn try_duplicate(&self, player: &Player, hand: InteractionHand, held: &ItemStack) -> bool {
        if !self.is_dancing() || !self.can_duplicate() {
            return false;
        }
        if !REGISTRY
            .items
            .is_in_tag(held.item(), &ItemTag::DUPLICATES_ALLAYS)
        {
            return false;
        }
        let Some(world) = self.level() else {
            return false;
        };
        if self.duplicate_allay(&world).is_none() {
            return false;
        }

        self.broadcast_entity_event(DUPLICATION_HEARTS_EVENT);
        world.play_sound_at(
            &sound_events::BLOCK_AMETHYST_BLOCK_CHIME,
            SoundSource::Neutral,
            self.position(),
            INTERACTION_SOUND_VOLUME,
            1.0,
            None,
        );
        player.inventory.lock().shrink_item_in_hand(hand, 1);
        true
    }

    /// Vanilla parity: the "give the allay an item" branch of `mobInteract`.
    fn take_item_from(&self, player: &Player, hand: InteractionHand, held: &ItemStack) {
        self.living_base
            .equipment()
            .lock()
            .set(EquipmentSlot::MainHand, held.copy_with_count(1));
        player.inventory.lock().shrink_item_in_hand(hand, 1);
        if let Some(world) = self.level() {
            world.play_sound_at(
                &sound_events::ENTITY_ALLAY_ITEM_GIVEN,
                SoundSource::Neutral,
                self.position(),
                INTERACTION_SOUND_VOLUME,
                1.0,
                None,
            );
        }
        self.brain
            .set_memory(memory_module_types::LIKED_PLAYER, player.uuid());
    }

    /// Vanilla parity: the "take the allay's item back" branch of `mobInteract`.
    ///
    /// Everything the allay had gathered is thrown at the player's feet, not
    /// handed over: only the one item it was holding goes into the inventory.
    fn give_item_back_to(&self, player: &Player, held_by_allay: ItemStack) {
        self.living_base
            .equipment()
            .lock()
            .set(EquipmentSlot::MainHand, ItemStack::empty());
        if let Some(world) = self.level() {
            world.play_sound_at(
                &sound_events::ENTITY_ALLAY_ITEM_TAKEN,
                SoundSource::Neutral,
                self.position(),
                INTERACTION_SOUND_VOLUME,
                1.0,
                None,
            );
        }
        self.swing(InteractionHand::MainHand, false);

        let position = self.position();
        for item in self.take_gathered_items() {
            throw_item(self, item, position);
        }

        self.brain
            .erase_memory(memory_module_types::LIKED_PLAYER.id());
        player.add_item_or_drop(held_by_allay);
    }

    /// Empties the carried container and hands back what was in it.
    ///
    /// Vanilla parity: `SimpleContainer.removeAllItems`, which both
    /// `mobInteract` and `dropEquipment` call.
    fn take_gathered_items(&self) -> Vec<ItemStack> {
        let mut inventory = self.inventory.lock();
        inventory
            .items_mut()
            .iter_mut()
            .filter(|item| !item.is_empty())
            .map(take)
            .collect()
    }
}

/// How a listener finds the allay it belongs to.
///
/// Vanilla parity: `EntityPositionSource(allay, allay.getEyeHeight())`, which
/// is what makes both of an allay's listeners report its eye rather than its
/// feet.
#[derive(Clone)]
struct ListenerSource {
    entity_id: i32,
    world: Weak<World>,
}

impl ListenerSource {
    fn eye_position(&self) -> Option<DVec3> {
        let world = self.world.upgrade()?;
        let entity = world.get_entity_by_id(self.entity_id)?;
        let allay = entity.downcast_ref::<AllayEntity>()?;
        Some(DVec3::new(
            allay.position().x,
            allay.get_eye_y(),
            allay.position().z,
        ))
    }

    /// Runs `visit` on the allay while the world still holds it.
    fn with_allay<R>(&self, visit: impl FnOnce(&AllayEntity) -> R) -> Option<R> {
        let world = self.world.upgrade()?;
        let entity = world.get_entity_by_id(self.entity_id)?;
        let allay = entity.downcast_ref::<AllayEntity>()?;
        Some(visit(allay))
    }
}

/// Vanilla parity: `Allay.JukeboxListener`, a plain game-event listener rather
/// than a vibration one -- an allay hears a jukebox directly.
struct JukeboxListener {
    allay: ListenerSource,
}

impl GameEventListener for JukeboxListener {
    fn listener_pos(&self) -> Option<DVec3> {
        self.allay.eye_position()
    }

    fn listener_radius(&self) -> i32 {
        vanilla_game_events::JUKEBOX_PLAY.notification_radius
    }

    fn handle_game_event(
        &self,
        _world: &Arc<World>,
        event: GameEventRef,
        _context: &GameEventContext<'_>,
        source_pos: DVec3,
    ) -> bool {
        let playing = if event.key == vanilla_game_events::JUKEBOX_PLAY.key {
            true
        } else if event.key == vanilla_game_events::JUKEBOX_STOP_PLAY.key {
            false
        } else {
            return false;
        };

        self.allay
            .with_allay(|allay| allay.set_jukebox_playing(BlockPos::from(source_pos), playing))
            .is_some()
    }
}

/// Vanilla `Allay.VibrationUser`.
///
/// An allay that has not been given a note block yet hears every one; once it likes one,
/// it hears only that one, and only while it is still within a thousand blocks of it.
struct AllayVibrationUser {
    allay: ListenerSource,
}

impl VibrationUser for AllayVibrationUser {
    fn listener_radius(&self) -> i32 {
        NOTE_BLOCK_LISTENER_RADIUS
    }

    fn position_source(&self) -> VibrationPositionSource {
        VibrationPositionSource::Entity {
            world: Weak::clone(&self.allay.world),
            entity_id: self.allay.entity_id,
            y_offset: self
                .allay
                .with_allay(|allay| allay.get_eye_height() as f32)
                .unwrap_or(0.0),
        }
    }

    fn listenable_events(&self) -> Identifier {
        GameEventTag::ALLAY_CAN_LISTEN
    }

    fn can_receive_vibration(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        _event: GameEventRef,
        _context: &GameEventContext<'_>,
    ) -> bool {
        self.allay
            .with_allay(|allay| {
                if allay.is_no_ai() {
                    return false;
                }
                let Some(liked) = allay
                    .brain
                    .get_memory(memory_module_types::LIKED_NOTEBLOCK_POSITION)
                else {
                    return true;
                };
                liked.dimension == world.key
                    && block_closer_than(liked.pos, allay.block_position(), LIKED_NOTEBLOCK_RANGE)
                    && liked.pos == pos
            })
            .unwrap_or(false)
    }

    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        event: GameEventRef,
        _source_entity: Option<&dyn Entity>,
        _projectile_owner: Option<&dyn Entity>,
        _receiving_distance: f32,
    ) {
        if event.key != vanilla_game_events::NOTE_BLOCK_PLAY.key {
            return;
        }
        self.allay.with_allay(|allay| {
            allay_ai::hear_noteblock(&allay.brain, world, pos);
        });
    }
}

/// Vanilla `GlobalPos.isCloseEnough`, which measures the block positions themselves.
fn block_closer_than(from: BlockPos, to: BlockPos, distance: f64) -> bool {
    let delta = from.0 - to.0;
    let distance_squared = f64::from(delta.x).mul_add(
        f64::from(delta.x),
        f64::from(delta.y).mul_add(f64::from(delta.y), f64::from(delta.z) * f64::from(delta.z)),
    );
    distance_squared < distance * distance
}

impl Entity for AllayEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    /// Vanilla parity: the server half of `Allay.tick`, which stops a panicking
    /// allay dancing.
    fn tick(&self) {
        // Vanilla parity: the `VibrationSystem.Ticker.tick` of `Allay.tick`, which is what
        // makes a note block heard across the room arrive a moment after it was struck.
        if let Some(world) = self.level() {
            self.ensure_listeners(&world);
            if let Some(listener) = self.vibration_listener() {
                listener.tick(&world);
            }
        }
        LivingEntity::tick_living_entity(self);
        if self.is_panicking() {
            self.set_dancing(false);
        }
    }

    /// Vanilla parity: `Allay.playStepSound`, which is empty -- an allay never
    /// touches the ground.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    /// Vanilla parity: `Allay.checkFallDamage`, which is empty. An allay that
    /// runs out of path mid-air lands unhurt.
    fn check_fall_damage(
        &self,
        _vertical_movement: f64,
        _on_ground: bool,
        _on_state: BlockStateId,
        _pos: BlockPos,
        _world: &Arc<World>,
    ) {
    }

    /// Vanilla parity: `Allay.considersEntityAsAlly`, which Foton reaches
    /// through `Entity.isAlliedTo` -- the same seam the tamed cat and the
    /// illagers use.
    fn is_allied_to(&self, other: &dyn Entity) -> bool {
        self.is_liked_player(Some(other))
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `Allay.updateDynamicGameEventListener`, which moves both
    /// of an allay's listeners together.
    fn update_dynamic_game_event_listener(
        &self,
        action: DynamicListenerAction,
        world: &Arc<World>,
    ) {
        if action == DynamicListenerAction::Add {
            self.ensure_listeners(world);
        }

        for listener in self.listeners.lock().iter() {
            listener.apply(action, world);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        save_inventory(&self.inventory.lock(), nbt);
        if let Some(listener) = self.vibration_listener() {
            let mut data = NbtCompound::new();
            listener.save(&mut data);
            nbt.insert(VIBRATION_DATA_TAG, data);
        }
        nbt.insert("DuplicationCooldown", self.duplication_cooldown());
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        load_inventory(&mut self.inventory.lock(), nbt);
        if let Some(world) = self.level() {
            self.ensure_listeners(&world);
        }
        if let Some(listener) = self.vibration_listener() {
            listener.load(nbt.compound(VIBRATION_DATA_TAG).as_ref());
        }
        self.set_duplication_cooldown(nbt.long("DuplicationCooldown").unwrap_or(0));
        self.brain.load(nbt);
    }
}

impl LivingEntity for AllayEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ALLAY_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ALLAY_DEATH)
    }

    fn sound_volume(&self) -> f32 {
        SOUND_VOLUME
    }

    /// Vanilla parity: `Allay.hurtServer`. The player it is fetching for cannot
    /// hurt it at all, which is what stops a stray swing losing you the allay.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let attacker = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id));
        if self.is_liked_player(attacker.as_deref()) {
            return false;
        }
        self.living_hurt_server(world, source, amount)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: the inventory half of `Allay.dropEquipment`. The item in
    /// hand is equipment, which Foton already drops through
    /// `Mob.dropCustomDeathLoot`; what is left is everything it had gathered.
    fn drop_custom_death_loot(&self, source: &DamageSource, killed_by_player: bool) {
        Mob::drop_custom_death_loot_mob(self, source, killed_by_player);

        let Some(world) = self.level() else {
            return;
        };
        let gathered = self.take_gathered_items();
        for item in gathered {
            world.spawn_item(self.position(), item);
        }
    }

    /// Vanilla parity: `Allay.aiStep`, which heals a heart every half second,
    /// stops the dance when the jukebox has gone, and counts the duplication
    /// cooldown down.
    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();

        if Entity::is_alive(self) && self.tick_count() % REGENERATION_INTERVAL == 0 {
            self.heal(REGENERATION_PER_HALF_SECOND);
        }
        if self.is_dancing()
            && self.should_stop_dancing()
            && self.tick_count() % DANCE_CHECK_INTERVAL == 0
        {
            self.set_dancing(false);
            *self.jukebox_pos.lock() = None;
        }
        self.update_duplication_cooldown();

        result
    }

    /// Vanilla parity: `Allay.travel`, which flies rather than walking.
    fn travel(&self, input: DVec3) -> Option<MoveResult> {
        self.travel_flying(input, self.get_speed())
    }
}

impl Mob for AllayEntity {
    /// Vanilla parity: `Mob.serverAiStep` ticks the goal selector for every
    /// mob it runs, brain-driven or not. `Mob::tick_goal_selectors` has an
    /// empty default, so leaving it out is how a registered goal set never
    /// runs.
    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Allay.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        allay_ai::update_activity(&self.brain);
    }

    fn move_control_kind(&self) -> MoveControlKind {
        MoveControlKind::Flying {
            max_turn: MOVE_CONTROL_MAX_TURN,
            hovers_in_place: true,
        }
    }

    /// Vanilla parity: `Allay.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if self.has_item_in_hand() {
            &sound_events::ENTITY_ALLAY_AMBIENT_WITH_ITEM
        } else {
            &sound_events::ENTITY_ALLAY_AMBIENT_WITHOUT_ITEM
        })
    }

    /// Vanilla parity: `Allay.canPickUpLoot`, which is what makes an empty-handed
    /// allay ignore the ground: it only fetches copies of what it is holding.
    fn can_pick_up_loot(&self) -> bool {
        !self.is_on_pickup_cooldown() && self.has_item_in_hand()
    }

    fn pickup_reach(&self) -> DVec3 {
        ITEM_PICKUP_REACH
    }

    /// Vanilla parity: `Allay.wantsToPickUp`.
    fn wants_to_pick_up(&self, world: &World, item_stack: &ItemStack) -> bool {
        let held = self.get_item_in_hand(InteractionHand::MainHand);
        !held.is_empty()
            && world.get_game_rule(&vanilla_game_rules::MOB_GRIEFING)
            && inventory_carrier::can_add_item(&self.inventory.lock(), item_stack)
            && Self::considers_item_equal(&held, item_stack)
    }

    /// Vanilla parity: `Allay.pickUpItem`.
    fn pick_up_item(&self, world: &Arc<World>, item_entity: &SharedEntity) {
        inventory_carrier::pick_up_item(world, self, item_entity);
    }

    /// Vanilla parity: `Allay.removeWhenFarAway`, a flat no -- an allay you have
    /// given something to is yours.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    // MISSING FOUNDATION: vanilla's `Allay.shouldStayCloseToLeashHolder` is
    // `false`, which is what lets a leashed allay fly out to fetch rather than
    // being dragged back. Foton's leash has no such seam, so a leashed allay
    // stays within the shared radius.
    /// Vanilla parity: `Allay.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let held = {
            let inventory = player.inventory.lock();
            let held = inventory.get_item_in_hand(hand);
            held.copy_with_count(held.count())
        };
        let item_in_hand = self.get_item_in_hand(InteractionHand::MainHand);

        if self.try_duplicate(player, hand, &held) {
            return InteractionResult::Success;
        }

        if item_in_hand.is_empty() && !held.is_empty() {
            self.take_item_from(player, hand, &held);
            return InteractionResult::Success;
        }

        if !item_in_hand.is_empty() && hand == InteractionHand::MainHand && held.is_empty() {
            self.give_item_back_to(player, item_in_hand);
            return InteractionResult::Success;
        }

        InteractionResult::Pass
    }
}

impl PathfinderMob for AllayEntity {
    /// Vanilla parity: `Allay.createNavigation`, a `FlyingPathNavigation`.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::Flying
    }
}

impl InventoryCarrier for AllayEntity {
    fn carried_inventory(&self) -> &SyncMutex<SimpleContainer> {
        &self.inventory
    }
}

/// Spawns an allay for a test or a command.
///
/// Vanilla parity: the plain `EntityType.ALLAY.create` an allay is otherwise
/// only made by, which the generated factory table already covers.
#[must_use]
pub fn spawn_allay(world: &Arc<World>, position: DVec3) -> Option<SharedEntity> {
    let entity = ENTITIES.create(
        &vanilla_entities::ALLAY,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    )?;
    entity.set_old_position_to_current();
    if let Some(mob) = entity.as_mob() {
        let _ = mob.finalize_spawn(world, EntitySpawnReason::Command, None);
    }
    world.try_add_entity(Arc::clone(&entity)).ok()?;
    Some(entity)
}

#[cfg(test)]
mod tests;
