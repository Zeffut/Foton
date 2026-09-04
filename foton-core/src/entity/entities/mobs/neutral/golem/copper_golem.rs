//! Copper golem entity.
//!
//! Vanilla parity: `CopperGolem`. New in 26.2. It oxidizes on a timer the way
//! a copper block does, and once it is fully oxidized it eventually seizes up
//! into a [`crate::block_entity::entities::CopperGolemStatueBlockEntity`].
//! Honeycomb stops the clock, an axe winds it back.
//!
//! Its whole job -- carrying one stack at a time out of a copper chest and into
//! an ordinary one -- is brain-driven, and lives in
//! [`super::copper_golem_ai`]. This entity has no goals at all; the brain in
//! [`Self::brain`] is its entire AI, exactly as in vanilla.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{
    BlockStateProperties, Direction as PropertyDirection, EnumProperty, Pose,
};
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::equipment::EquipmentSlot;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::CopperGolemEntityData;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, level_events, sound_events, vanilla_blocks,
    vanilla_game_events, vanilla_game_rules, vanilla_items,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::{InteractionHand, UpdateFlags};
use foton_utils::{
    BlockPos, BlockStateId, Direction, Downcast as _, DowncastType, DowncastTypeKey,
};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::behavior::blocks::WeatherState;
use crate::block_entity::entities::CopperGolemStatueBlockEntity;
use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, RemovalReason, SpawnGroupData,
};
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

use super::{AMBIENT_SOUND_INTERVAL, copper_golem_ai};

/// Marks a golem waxed with honeycomb, which never oxidizes again.
///
/// Vanilla parity: `CopperGolem.IGNORE_WEATHERING_TICK`.
const IGNORE_WEATHERING_TICK: i64 = -2;

/// Marks a golem whose next oxidation has not been rolled yet.
///
/// Vanilla parity: `CopperGolem.UNSET_WEATHERING_TICK`.
const UNSET_WEATHERING_TICK: i64 = -1;

/// Shortest wait between oxidation stages.
///
/// Vanilla parity: `CopperGolem.WEATHERING_TICK_FROM`.
const WEATHERING_TICK_FROM: i64 = 504_000;

/// Longest wait between oxidation stages.
///
/// Vanilla parity: `CopperGolem.WEATHERING_TICK_TO`.
const WEATHERING_TICK_TO: i64 = 552_000;

/// Chance per tick that a fully oxidized golem seizes up.
///
/// Vanilla parity: `CopperGolem.TURN_TO_STATUE_CHANCE`.
const TURN_TO_STATUE_CHANCE: f32 = 0.0058;

/// How high above the ground the golem drops its antenna when sheared.
///
/// Vanilla parity: the `1.5F` of `CopperGolem.shear`.
const ANTENNA_DROP_HEIGHT: f64 = 1.5;

/// The slot the copper golem wears its antenna in.
///
/// Vanilla parity: `CopperGolem.EQUIPMENT_SLOT_ANTENNA`.
pub const EQUIPMENT_SLOT_ANTENNA: EquipmentSlot = EquipmentSlot::Saddle;

/// How far the golem can path.
///
/// Vanilla parity: the `setRequiredPathLength(48.0F)` of the constructor.
const REQUIRED_PATH_LENGTH: f32 = 48.0;

/// How badly the golem wants to stay away from fire.
///
/// Vanilla parity: the pathfinding maluses of the constructor.
const NEIGHBORING_DANGER_MALUS: f32 = 16.0;
const FIRE_MALUS: f32 = -1.0;

/// What a copper golem is doing with its hands.
///
/// Vanilla parity: `CopperGolemState`. Every state but [`Self::Idle`] belongs
/// to the chest behaviors Foton cannot run yet, but the value is synced, so the
/// enum is here in full rather than reduced to what the server sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopperGolemState {
    /// Standing about.
    Idle,
    /// Taking an item out of a container.
    GettingItem,
    /// Reaching into an empty container.
    GettingNoItem,
    /// Putting an item into a container.
    DroppingItem,
    /// Reaching into a container with nothing to put in it.
    DroppingNoItem,
}

impl CopperGolemState {
    /// Returns the protocol id of this state.
    #[must_use]
    pub const fn id(self) -> i32 {
        match self {
            Self::Idle => 0,
            Self::GettingItem => 1,
            Self::GettingNoItem => 2,
            Self::DroppingItem => 3,
            Self::DroppingNoItem => 4,
        }
    }

    /// Returns the state with this protocol id.
    ///
    /// Vanilla parity: the `ByIdMap.continuous(..., ZERO)` of
    /// `CopperGolemState.BY_ID`, which falls back to idle.
    #[must_use]
    pub const fn by_id(id: i32) -> Self {
        match id {
            1 => Self::GettingItem,
            2 => Self::GettingNoItem,
            3 => Self::DroppingItem,
            4 => Self::DroppingNoItem,
            _ => Self::Idle,
        }
    }
}

/// The block property a statue's pose lives in.
const COPPER_GOLEM_POSE: &EnumProperty<Pose> = &BlockStateProperties::COPPER_GOLEM_POSE;

/// The block property a statue's facing lives in.
const STATUE_FACING: &EnumProperty<PropertyDirection> = &BlockStateProperties::HORIZONTAL_FACING;

/// Mutable state vanilla keeps as plain fields on the golem.
#[derive(Debug)]
struct CopperGolemInternalState {
    /// Game time the next oxidation is due at, or one of the two sentinels.
    next_weathering_tick: i64,
    /// The last bolt that hit, so one strike only scrapes one stage off.
    last_lightning_bolt_uuid: Option<Uuid>,
    /// The chest the golem currently has its arm in, if any.
    ///
    /// Vanilla parity: `CopperGolem.openedChestPos`.
    opened_chest_pos: Option<BlockPos>,
}

/// A copper golem.
#[entity_behavior(class = "CopperGolem")]
pub struct CopperGolemEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<CopperGolemEntityData>,
    state: SyncMutex<CopperGolemInternalState>,
    brain: Brain,
}

// SAFETY: This key is owned by Foton and uniquely identifies `CopperGolemEntity`.
unsafe impl DowncastType for CopperGolemEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/copper_golem");
}

impl CopperGolemEntity {
    /// Creates a copper golem at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a copper golem from saved base data.
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
        let mut entity_data = CopperGolemEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            let mut navigation = mob_base.navigation().lock();
            let follow_range = f64::from(REQUIRED_PATH_LENGTH);
            navigation.set_required_path_length(REQUIRED_PATH_LENGTH, follow_range);
            navigation.set_can_open_doors(true);
        }

        let golem = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(CopperGolemInternalState {
                next_weathering_tick: UNSET_WEATHERING_TICK,
                last_lightning_bolt_uuid: None,
                opened_chest_pos: None,
            }),
            brain: copper_golem_ai::make_brain(),
        };

        // Vanilla `CopperGolem`'s constructor.
        golem.set_persistence_required();
        golem.set_pathfinding_malus(PathType::FireInNeighbor, NEIGHBORING_DANGER_MALUS);
        golem.set_pathfinding_malus(PathType::DamagingInNeighbor, NEIGHBORING_DANGER_MALUS);
        golem.set_pathfinding_malus(PathType::Fire, FIRE_MALUS);
        golem
    }

    /// Returns what the golem is doing with its hands.
    ///
    /// Vanilla parity: `CopperGolem.getState`.
    #[must_use]
    pub fn state(&self) -> CopperGolemState {
        CopperGolemState::by_id(
            *self
                .entity_data
                .lock()
                .copper_golem()
                .copper_golem_state
                .get(),
        )
    }

    /// Records what the golem is doing with its hands.
    ///
    /// Vanilla parity: `CopperGolem.setState`.
    pub fn set_state(&self, state: CopperGolemState) {
        self.entity_data
            .lock()
            .copper_golem_mut()
            .copper_golem_state
            .set(state.id());
    }

    /// Returns the chest the golem currently has open, if any.
    ///
    /// Vanilla parity: the `openedChestPos` field `CopperGolem.hasContainerOpen`
    /// answers from.
    #[must_use]
    pub fn opened_chest_pos(&self) -> Option<BlockPos> {
        self.state.lock().opened_chest_pos
    }

    /// Records which chest the golem has open.
    ///
    /// Vanilla parity: `CopperGolem.setOpenedChestPos`.
    pub fn set_opened_chest_pos(&self, pos: BlockPos) {
        self.state.lock().opened_chest_pos = Some(pos);
    }

    /// Forgets whichever chest the golem had open.
    ///
    /// Vanilla parity: `CopperGolem.clearOpenedChestPos`.
    pub fn clear_opened_chest_pos(&self) {
        self.state.lock().opened_chest_pos = None;
    }

    /// Returns how oxidized the golem is.
    ///
    /// Vanilla parity: `CopperGolem.getWeatherState`.
    #[must_use]
    pub fn weather_state(&self) -> WeatherState {
        WeatherState::by_id(*self.entity_data.lock().copper_golem().weather_state.get())
    }

    /// Records how oxidized the golem is.
    ///
    /// Vanilla parity: `CopperGolem.setWeatherState`.
    pub fn set_weather_state(&self, weather_state: WeatherState) {
        self.entity_data
            .lock()
            .copper_golem_mut()
            .weather_state
            .set(weather_state.id());
    }

    /// Sets the golem's starting oxidation and announces it.
    ///
    /// Vanilla parity: `CopperGolem.spawn`, called by the carved pumpkin with
    /// whatever copper the golem was built from.
    pub fn spawn(&self, weather_state: WeatherState) {
        self.set_weather_state(weather_state);
        self.play_spawn_sound();
    }

    /// Vanilla parity: `CopperGolem.playSpawnSound`.
    pub fn play_spawn_sound(&self) {
        self.play_sound(&sound_events::ENTITY_COPPER_GOLEM_SPAWN, 1.0, 1.0);
    }

    /// Returns whether shears would take the antenna off.
    ///
    /// Vanilla parity: `CopperGolem.readyForShearing`.
    #[must_use]
    pub fn ready_for_shearing(&self) -> bool {
        let mut shearable = false;
        self.with_equipment_slot(EQUIPMENT_SLOT_ANTENNA, &mut |item| {
            shearable = REGISTRY
                .items
                .is_in_tag(item.item(), &ItemTag::SHEARABLE_FROM_COPPER_GOLEM);
        });
        shearable
    }

    /// Takes the antenna off and drops it.
    ///
    /// Vanilla parity: `CopperGolem.shear`.
    pub fn shear(&self, world: &Arc<World>) {
        world.play_sound_at(
            &sound_events::ENTITY_COPPER_GOLEM_SHEAR,
            SoundSource::Players,
            self.position(),
            1.0,
            1.0,
            None,
        );

        let mut antenna = ItemStack::empty();
        self.with_equipment_slot_mut(EQUIPMENT_SLOT_ANTENNA, &mut |item| {
            antenna = item.copy_with_count(item.count());
            *item = ItemStack::empty();
        });
        let _ = self.spawn_at_location(antenna, ANTENNA_DROP_HEIGHT);
    }

    /// Advances the oxidation clock, and freezes the golem when it runs out.
    ///
    /// Vanilla parity: `CopperGolem.updateWeathering`.
    fn update_weathering(&self, world: &Arc<World>) {
        let game_time = world.game_time();
        let next_tick = self.state.lock().next_weathering_tick;
        if next_tick == IGNORE_WEATHERING_TICK {
            return;
        }

        if next_tick == UNSET_WEATHERING_TICK {
            self.state.lock().next_weathering_tick =
                game_time + rand::random_range(WEATHERING_TICK_FROM..=WEATHERING_TICK_TO);
            return;
        }

        let weather_state = self.weather_state();
        let is_fully_oxidized = weather_state == WeatherState::Oxidized;
        if game_time >= next_tick && !is_fully_oxidized {
            let new_state = weather_state.next();
            self.set_weather_state(new_state);
            self.state.lock().next_weathering_tick = if new_state == WeatherState::Oxidized {
                0
            } else {
                next_tick + rand::random_range(WEATHERING_TICK_FROM..=WEATHERING_TICK_TO)
            };
        }

        if is_fully_oxidized && self.can_turn_to_statue(world) {
            self.turn_to_statue(world);
        }
    }

    /// Vanilla parity: `CopperGolem.canTurnToStatue`.
    fn can_turn_to_statue(&self, world: &Arc<World>) -> bool {
        world.get_block_state(self.block_position()).is_air()
            && rand::random::<f32>() <= TURN_TO_STATUE_CHANCE
    }

    /// Freezes the golem into a statue block.
    ///
    /// Vanilla parity: `CopperGolem.turnToStatue`.
    fn turn_to_statue(&self, world: &Arc<World>) {
        let pos = self.block_position();
        let poses = COPPER_GOLEM_POSE.possible_values;
        let pose = poses[rand::random_range(0..poses.len())].clone();
        let statue = vanilla_blocks::OXIDIZED_COPPER_GOLEM_STATUE
            .default_state()
            .set_value(COPPER_GOLEM_POSE, pose)
            .set_value(STATUE_FACING, Direction::from_yaw(self.rotation().0));
        world.set_block(pos, statue, UpdateFlags::UPDATE_ALL);

        let Some(block_entity) = world.get_block_entity(pos) else {
            return;
        };
        let Some(statue_entity) = block_entity.downcast_ref::<CopperGolemStatueBlockEntity>()
        else {
            return;
        };

        statue_entity.create_statue(self.custom_name());
        self.drop_preserved_equipment(world);
        self.play_sound(&sound_events::ENTITY_COPPER_GOLEM_BECOME_STATUE, 1.0, 1.0);
        if self.is_leashed() {
            if world.get_game_rule(&vanilla_game_rules::ENTITY_DROPS) {
                self.drop_leash();
            } else {
                self.remove_leash();
            }
        }
        self.set_removed(RemovalReason::Discarded);
    }

    /// Drops the equipment a statue leaves behind.
    ///
    /// Vanilla parity: `Mob.dropPreservedEquipment`, restricted to the antenna
    /// because that is the only slot a copper golem uses.
    fn drop_preserved_equipment(&self, _world: &Arc<World>) {
        if !self.is_equipment_drop_preserved(EQUIPMENT_SLOT_ANTENNA) {
            return;
        }

        let mut antenna = ItemStack::empty();
        self.with_equipment_slot_mut(EQUIPMENT_SLOT_ANTENNA, &mut |item| {
            antenna = item.copy_with_count(item.count());
            *item = ItemStack::empty();
        });
        if !antenna.is_empty() {
            let _ = self.spawn_at_location(antenna, 0.0);
        }
    }

    /// Returns the sounds this golem makes at its current oxidation.
    ///
    /// Vanilla parity: `CopperGolemOxidationLevels.getOxidationLevel`, minus
    /// the two textures, which are client-side.
    fn oxidation_sounds(&self) -> OxidationSounds {
        match self.weather_state() {
            WeatherState::Unaffected | WeatherState::Exposed => OxidationSounds {
                hurt: &sound_events::ENTITY_COPPER_GOLEM_HURT,
                death: &sound_events::ENTITY_COPPER_GOLEM_DEATH,
                step: &sound_events::ENTITY_COPPER_GOLEM_STEP,
            },
            WeatherState::Weathered => OxidationSounds {
                hurt: &sound_events::ENTITY_COPPER_GOLEM_WEATHERED_HURT,
                death: &sound_events::ENTITY_COPPER_GOLEM_WEATHERED_DEATH,
                step: &sound_events::ENTITY_COPPER_GOLEM_WEATHERED_STEP,
            },
            WeatherState::Oxidized => OxidationSounds {
                hurt: &sound_events::ENTITY_COPPER_GOLEM_OXIDIZED_HURT,
                death: &sound_events::ENTITY_COPPER_GOLEM_OXIDIZED_DEATH,
                step: &sound_events::ENTITY_COPPER_GOLEM_OXIDIZED_STEP,
            },
        }
    }

    /// Handles the honeycomb and axe half of `CopperGolem.mobInteract`.
    fn interact_with_tool(
        &self,
        world: &Arc<World>,
        player: &Player,
        hand: InteractionHand,
        item_stack: &ItemStack,
    ) -> InteractionResult {
        let next_weathering_tick = self.state.lock().next_weathering_tick;

        if item_stack.is(&vanilla_items::HONEYCOMB)
            && next_weathering_tick != IGNORE_WEATHERING_TICK
        {
            world.level_event(
                level_events::PARTICLES_AND_SOUND_WAX_ON,
                self.block_position(),
                0,
                None,
            );
            self.state.lock().next_weathering_tick = IGNORE_WEATHERING_TICK;
            self.use_player_item(player, hand);
            return InteractionResult::SuccessServer;
        }

        if !REGISTRY.items.is_in_tag(item_stack.item(), &ItemTag::AXES) {
            return InteractionResult::Pass;
        }

        if next_weathering_tick == IGNORE_WEATHERING_TICK {
            world.play_sound_at(
                &sound_events::ITEM_AXE_WAX_OFF,
                self.sound_source(),
                self.position(),
                1.0,
                1.0,
                None,
            );
            world.level_event(
                level_events::PARTICLES_WAX_OFF,
                self.block_position(),
                0,
                None,
            );
            self.state.lock().next_weathering_tick = UNSET_WEATHERING_TICK;
            player.hurt_item_in_hand(hand, 1);
            return InteractionResult::SuccessServer;
        }

        let weather_state = self.weather_state();
        if weather_state == WeatherState::Unaffected {
            return InteractionResult::Pass;
        }

        world.play_sound_at(
            &sound_events::ITEM_AXE_SCRAPE,
            self.sound_source(),
            self.position(),
            1.0,
            1.0,
            None,
        );
        world.level_event(
            level_events::PARTICLES_SCRAPE,
            self.block_position(),
            0,
            None,
        );
        self.state.lock().next_weathering_tick = UNSET_WEATHERING_TICK;
        self.set_weather_state(weather_state.previous());
        player.hurt_item_in_hand(hand, 1);
        InteractionResult::SuccessServer
    }
}

/// The sound set one oxidation stage uses.
struct OxidationSounds {
    hurt: SoundEventRef,
    death: SoundEventRef,
    step: SoundEventRef,
}

impl Entity for CopperGolemEntity {
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

    /// Vanilla parity: `CopperGolem.tick`, whose server half is the weathering
    /// clock; the client half only sets up animation states.
    fn tick(&self) {
        LivingEntity::tick_living_entity(self);
        if let Some(world) = self.level() {
            self.update_weathering(&world);
        }
    }

    /// Vanilla parity: `CopperGolem.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(self.oxidation_sounds().step, 1.0, 1.0);
    }

    /// Vanilla parity: `CopperGolem.thunderHit`, which scrapes a stage off.
    fn thunder_hit(&self, world: &World, bolt: &dyn Entity) {
        self.entity_thunder_hit(world);

        let bolt_uuid = bolt.uuid();
        if self.state.lock().last_lightning_bolt_uuid == Some(bolt_uuid) {
            return;
        }
        self.state.lock().last_lightning_bolt_uuid = Some(bolt_uuid);

        let weather_state = self.weather_state();
        if weather_state == WeatherState::Unaffected {
            return;
        }
        self.state.lock().next_weathering_tick = UNSET_WEATHERING_TICK;
        self.set_weather_state(weather_state.previous());
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        // Vanilla parity: `LivingEntity.addAdditionalSaveData` writes the brain
        // for every living entity; only mobs that have one do here.
        self.brain.save(nbt);
        nbt.insert("next_weather_age", self.state.lock().next_weathering_tick);
        nbt.insert("weather_state", weather_state_name(self.weather_state()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.brain.load(nbt);
        self.state.lock().next_weathering_tick = nbt
            .long("next_weather_age")
            .unwrap_or(UNSET_WEATHERING_TICK);
        let weather_state = nbt
            .string("weather_state")
            .and_then(|name| weather_state_by_name(&name.to_str()))
            .unwrap_or(WeatherState::Unaffected);
        self.set_weather_state(weather_state);
    }
}

/// Returns the name vanilla's `WeatherState.CODEC` writes.
const fn weather_state_name(weather_state: WeatherState) -> &'static str {
    match weather_state {
        WeatherState::Unaffected => "unaffected",
        WeatherState::Exposed => "exposed",
        WeatherState::Weathered => "weathered",
        WeatherState::Oxidized => "oxidized",
    }
}

/// Returns the stage vanilla's `WeatherState.CODEC` reads.
fn weather_state_by_name(name: &str) -> Option<WeatherState> {
    match name {
        "unaffected" => Some(WeatherState::Unaffected),
        "exposed" => Some(WeatherState::Exposed),
        "weathered" => Some(WeatherState::Weathered),
        "oxidized" => Some(WeatherState::Oxidized),
        _ => None,
    }
}

impl LivingEntity for CopperGolemEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
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

    /// Vanilla parity: the `Mob.serverAiStep` the copper golem inherits, which
    /// is what actually reaches [`Mob::custom_server_ai_step`] and so the brain.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(self.oxidation_sounds().hurt)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(self.oxidation_sounds().death)
    }

    /// Vanilla parity: `CopperGolem.actuallyHurt`, which drops whatever the
    /// golem was in the middle of.
    fn actually_hurt(&self, world: &World, source: &DamageSource, amount: f32) {
        self.living_actually_hurt(world, source, amount);
        self.set_state(CopperGolemState::Idle);
    }
}

impl Mob for CopperGolemEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    /// Vanilla parity: `CopperGolem.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        copper_golem_ai::update_activity(&self.brain);
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `AbstractGolem.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        None
    }

    fn ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `AbstractGolem.removeWhenFarAway`.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    /// Vanilla parity: `CopperGolem.finalizeSpawn`.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.play_spawn_sound();
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `CopperGolem.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        let Some(world) = self.level() else {
            return InteractionResult::Pass;
        };

        if item_stack.is_empty() && self.throw_held_item_to(&world, player.position()) {
            return InteractionResult::Success;
        }

        if item_stack.is(&vanilla_items::SHEARS) && self.ready_for_shearing() {
            self.shear(&world);
            world.game_event_at(
                &vanilla_game_events::SHEAR,
                self.position(),
                &GameEventContext::new(Some(player as &dyn Entity), None),
            );
            player.hurt_item_in_hand(hand, 1);
            return InteractionResult::Success;
        }

        self.interact_with_tool(&world, player, hand, &item_stack)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl CopperGolemEntity {
    /// Tosses whatever the golem is holding towards `target`.
    ///
    /// Vanilla parity: the `BehaviorUtils.throwItem` branch of
    /// `CopperGolem.mobInteract`.
    fn throw_held_item_to(&self, world: &Arc<World>, target: DVec3) -> bool {
        let mut held = ItemStack::empty();
        self.with_equipment_slot_mut(EquipmentSlot::MainHand, &mut |item| {
            held = item.copy_with_count(item.count());
            *item = ItemStack::empty();
        });
        if held.is_empty() {
            return false;
        }

        let position = self.position();
        let hand_y = self.get_eye_y() - THROW_HAND_Y_OFFSET;
        let velocity = (target - position).normalize_or_zero() * THROW_VELOCITY;
        let _ = world.spawn_item_with_velocity(
            DVec3::new(position.x, hand_y, position.z),
            held,
            velocity,
        );
        true
    }
}

/// How far below the eyes the golem's hand is when it throws.
///
/// Vanilla parity: the `0.3F` hand offset of `BehaviorUtils.throwItem`.
const THROW_HAND_Y_OFFSET: f64 = 0.3;

/// How hard the golem tosses an item back.
///
/// Vanilla parity: the `new Vec3(0.3F, 0.3F, 0.3F)` of `BehaviorUtils.throwItem`.
const THROW_VELOCITY: f64 = 0.3;

impl PathfinderMob for CopperGolemEntity {}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use std::string::ToString;

    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use simdnbt::borrow::read_compound as read_borrowed_compound;

    use super::*;

    fn copper_golem() -> CopperGolemEntity {
        init_vanilla_registry();
        CopperGolemEntity::new(
            &vanilla_entities::COPPER_GOLEM,
            1,
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        )
    }

    fn load_from(nbt: &NbtCompound) -> CopperGolemEntity {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let golem = copper_golem();
        golem.load_additional((&borrowed).into());
        golem
    }

    #[test]
    fn a_waxed_golem_saves_the_sentinel_that_stops_its_clock() {
        let golem = copper_golem();
        golem.set_weather_state(WeatherState::Exposed);
        golem.state.lock().next_weathering_tick = IGNORE_WEATHERING_TICK;

        let mut nbt = NbtCompound::new();
        golem.save_additional(&mut nbt);
        assert_eq!(nbt.long("next_weather_age"), Some(IGNORE_WEATHERING_TICK));
        assert_eq!(
            nbt.string("weather_state").map(ToString::to_string),
            Some("exposed".to_owned())
        );

        let loaded = load_from(&nbt);
        assert_eq!(loaded.weather_state(), WeatherState::Exposed);
        assert_eq!(
            loaded.state.lock().next_weathering_tick,
            IGNORE_WEATHERING_TICK
        );
    }

    #[test]
    fn a_golem_with_no_saved_oxidation_starts_fresh_with_an_unrolled_clock() {
        init_vanilla_registry();
        let loaded = load_from(&NbtCompound::new());
        assert_eq!(loaded.weather_state(), WeatherState::Unaffected);
        assert_eq!(
            loaded.state.lock().next_weathering_tick,
            UNSET_WEATHERING_TICK
        );
    }

    #[test]
    fn an_unknown_state_id_falls_back_to_idle() {
        // Vanilla's `CopperGolemState.BY_ID` uses the `ZERO` out-of-bounds
        // strategy, so a client-side value Foton does not know reads as idle.
        assert_eq!(CopperGolemState::by_id(-1), CopperGolemState::Idle);
        assert_eq!(CopperGolemState::by_id(9), CopperGolemState::Idle);
        assert_eq!(
            CopperGolemState::by_id(CopperGolemState::DroppingItem.id()),
            CopperGolemState::DroppingItem
        );
    }
}
