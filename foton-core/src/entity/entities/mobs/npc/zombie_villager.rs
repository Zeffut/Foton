//! The zombie villager, and the cure.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.zombie.ZombieVillager`.
//!
//! This is the other half of what makes a villager worth anything: a zombie
//! villager keeps the profession, trades, experience and gossip of whoever it
//! used to be, and feeding it a golden apple while it is weakened turns it back
//! -- permanently cheaper, because the cure writes a `MAJOR_POSITIVE` gossip
//! that never decays. Iron bars and beds nearby speed the conversion up, which
//! is the entire shape of a cure farm.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::BlockRef;
use foton_registry::entity_type::{EntityDimensions, EntityTypeRef};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::trading::{MerchantOffers, offer_nbt};
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_entity_data::ZombieVillagerEntityData;
use foton_registry::villager_profession::VillagerProfessionRef;
use foton_registry::villager_type::VillagerTypeRef;
use foton_registry::{
    REGISTRY, RegistryEntry as _, RegistryExt as _, sound_events, vanilla_blocks, vanilla_entities,
    vanilla_items, vanilla_mob_effects, vanilla_villager_professions, vanilla_villager_types,
};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::random::legacy_random::LegacyRandom;
use foton_utils::types::{Difficulty, InteractionHand};
use foton_utils::{BlockPos, DowncastType, DowncastTypeKey, Identifier, UuidExt as _};
use glam::DVec3;
use rand::RngExt as _;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use std::str::FromStr as _;
use uuid::Uuid;

use super::super::hostile::zombie_common;
use crate::behavior::InteractionResult;
use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::goal::{
    HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::ai::gossip::{GossipContainer, ReputationEventType};
use crate::entity::conversion::ConversionReason::Cured;
use crate::entity::conversion::{ConversionParams, convert_to};
use crate::entity::damage::DamageSource;
use crate::entity::entities::VillagerEntity;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

/// Vanilla parity: `ZombieVillager.VILLAGER_CONVERSION_WAIT_MIN`.
const CONVERSION_WAIT_MIN: i32 = 3_600;
/// Vanilla parity: the `nextInt(2401)` of `mobInteract`, which with the minimum
/// gives the 3600..=6000 tick window `VILLAGER_CONVERSION_WAIT_MAX` names.
const CONVERSION_WAIT_SPREAD: i32 = 2_401;
/// Vanilla parity: `ZombieVillager.MAX_SPECIAL_BLOCKS_COUNT`.
const MAX_SPECIAL_BLOCKS: i32 = 14;
/// Vanilla parity: `ZombieVillager.SPECIAL_BLOCK_RADIUS`.
const SPECIAL_BLOCK_RADIUS: i32 = 4;
/// Vanilla parity: `ZombieVillager.NOT_CONVERTING`.
const NOT_CONVERTING: i32 = -1;
/// Vanilla parity: the `200` ticks of nausea a cured villager wakes up with.
const CURE_NAUSEA_TICKS: i32 = 200;
/// Vanilla parity: the `1027` level event a finished cure plays.
const CURE_LEVEL_EVENT: i32 = 1027;

/// Vanilla parity: `ZombieVillager.BABY_DIMENSIONS`, which match the
/// `baby_dimensions` of the extracted `entities.json` entry.
const BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new(0.49, 0.98, 0.67);

/// A villager that was bitten, and can be given back.
#[entity_behavior(class = "ZombieVillager")]
pub struct ZombieVillagerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<ZombieVillagerEntityData>,
    /// Ticks left before the cure completes, or [`NOT_CONVERTING`].
    villager_conversion_time: SyncMutex<i32>,
    /// Who fed it the golden apple, so they get the reputation for it.
    conversion_starter: SyncMutex<Option<Uuid>>,
    /// The gossip it had as a villager, kept intact through the bite.
    gossips: SyncMutex<Option<GossipContainer>>,
    /// The trades it had as a villager, likewise.
    trade_offers: SyncMutex<Option<MerchantOffers>>,
    villager_xp: SyncMutex<i32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `ZombieVillagerEntity`.
unsafe impl DowncastType for ZombieVillagerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/zombie_villager");
}

/// Speed multiplier a zombie villager uses while chasing.
///
/// Vanilla parity: the `ZombieAttackGoal(this, 1.0, false)` of
/// `Zombie.addBehaviourGoals`, which a zombie villager inherits.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Distance at which a zombie villager turns to watch a player.
///
/// Vanilla parity: `LookAtPlayerGoal(this, Player.class, 8.0F)`.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Speed multiplier for aimless wandering.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

impl ZombieVillagerEntity {
    /// Creates a zombie villager at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs a zombie villager from persisted base entity state.
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
        // Vanilla parity: `ZombieVillager.initializeZombieVillagerData` picks a
        // random profession, which the generated defaults already do -- so a
        // naturally spawned zombie villager arrives wearing a trade's clothes.
        let mut random = LegacyRandom::from_seed(rand::random());
        let mut entity_data = ZombieVillagerEntityData::new(&mut random);
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: `ZombieVillager` never overrides `registerGoals`,
            // so it gets `Zombie`'s set exactly -- the same one the husk, the
            // drowned and the zombified piglin each carry. Registering none of
            // them left a zombie villager ticking an empty goal list: it never
            // chased, never swung and never wandered.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(3, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(7, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(8, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(8, RandomLookAroundGoal::new());
        }

        {
            // Vanilla parity: the zombie's targetSelector.
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            villager_conversion_time: SyncMutex::new(NOT_CONVERTING),
            conversion_starter: SyncMutex::new(None),
            gossips: SyncMutex::new(None),
            trade_offers: SyncMutex::new(None),
            villager_xp: SyncMutex::new(0),
        }
    }

    /// Whether this is a baby zombie villager.
    ///
    /// Vanilla parity: `Zombie.isBaby`, which a zombie villager inherits.
    #[must_use]
    pub fn is_baby(&self) -> bool {
        *self.entity_data.lock().zombie.baby.get()
    }

    /// Vanilla parity: `VillagerData.type`.
    #[must_use]
    pub fn villager_type(&self) -> VillagerTypeRef {
        let id = self.entity_data.lock().villager_data.get().villager_type;
        usize::try_from(id)
            .ok()
            .and_then(|id| REGISTRY.villager_types.by_id(id))
            .unwrap_or(&vanilla_villager_types::PLAINS)
    }

    /// Vanilla parity: `VillagerData.profession`.
    #[must_use]
    pub fn profession(&self) -> VillagerProfessionRef {
        let id = self.entity_data.lock().villager_data.get().profession;
        usize::try_from(id)
            .ok()
            .and_then(|id| REGISTRY.villager_professions.by_id(id))
            .unwrap_or(&vanilla_villager_professions::NONE)
    }

    /// Vanilla parity: `VillagerData.level`.
    #[must_use]
    pub fn villager_level(&self) -> i32 {
        self.entity_data.lock().villager_data.get().level
    }

    /// Vanilla parity: `VillagerDataHolder.getVillagerDataFinalized`.
    #[must_use]
    pub fn villager_data_finalized(&self) -> bool {
        *self.entity_data.lock().villager_data_finalized.get()
    }

    /// Vanilla parity: `VillagerDataHolder.setVillagerDataFinalized`.
    pub fn set_villager_data_finalized(&self, finalized: bool) {
        self.entity_data
            .lock()
            .villager_data_finalized
            .set(finalized);
    }

    /// Sets the biome variant this zombie villager will be cured back into.
    pub fn set_villager_type(&self, villager_type: VillagerTypeRef) {
        let Ok(id) = i32::try_from(villager_type.id()) else {
            return;
        };
        let mut data = self.entity_data.lock();
        let mut villager_data = *data.villager_data.get();
        villager_data.villager_type = id;
        data.villager_data.set(villager_data);
    }

    /// Sets the profession, forgetting the trades of the old one.
    ///
    /// Vanilla parity: `ZombieVillager.setVillagerData`, which drops the kept
    /// offers when the profession changed, exactly as `Villager` does.
    pub fn set_profession(&self, profession: VillagerProfessionRef) {
        let Ok(id) = i32::try_from(profession.id()) else {
            return;
        };
        let changed = {
            let mut data = self.entity_data.lock();
            let mut villager_data = *data.villager_data.get();
            let changed = villager_data.profession != id;
            villager_data.profession = id;
            data.villager_data.set(villager_data);
            changed
        };
        if changed {
            *self.trade_offers.lock() = None;
        }
    }

    /// Vanilla parity: `VillagerData.level`.
    pub fn set_villager_level(&self, level: i32) {
        let mut data = self.entity_data.lock();
        let mut villager_data = *data.villager_data.get();
        villager_data.level = level.max(1);
        data.villager_data.set(villager_data);
    }

    /// Vanilla parity: `ZombieVillager.setTradeOffers`.
    pub fn set_trade_offers(&self, offers: MerchantOffers) {
        *self.trade_offers.lock() = Some(offers);
    }

    /// Vanilla parity: `ZombieVillager.setGossips`.
    pub fn set_gossips(&self, gossips: GossipContainer) {
        *self.gossips.lock() = Some(gossips);
    }

    /// Vanilla parity: `ZombieVillager.setVillagerXp`.
    pub fn set_villager_xp(&self, xp: i32) {
        *self.villager_xp.lock() = xp;
    }

    /// Vanilla parity: `ZombieVillager.getVillagerXp`.
    #[must_use]
    pub fn villager_xp(&self) -> i32 {
        *self.villager_xp.lock()
    }

    /// Whether a cure is under way.
    ///
    /// Vanilla parity: `ZombieVillager.isConverting`, which is the synched
    /// boolean the client uses to shake the zombie.
    #[must_use]
    pub fn is_converting(&self) -> bool {
        *self.entity_data.lock().converting.get()
    }

    /// Ticks left before the cure completes.
    #[must_use]
    pub fn conversion_time(&self) -> i32 {
        *self.villager_conversion_time.lock()
    }

    /// Starts the cure.
    ///
    /// Vanilla parity: `ZombieVillager.startConverting`. The weakness that
    /// allowed it is removed and replaced with strength for the whole wait,
    /// which is why a curing zombie villager hits harder than a plain one.
    pub fn start_converting(&self, player: Option<Uuid>, time: i32) {
        *self.conversion_starter.lock() = player;
        *self.villager_conversion_time.lock() = time;
        self.entity_data.lock().converting.set(true);
        self.living_base
            .remove_mob_effect(vanilla_mob_effects::WEAKNESS);

        let difficulty = self
            .level()
            .map_or(Difficulty::Normal, |world| world.difficulty());
        // Vanilla parity: `Math.min(difficulty.getId() - 1, 0)`, which really is
        // a minimum -- so the amplifier is zero on every difficulty but
        // peaceful, where it would be negative.
        let amplifier = (difficulty_id(difficulty) - 1).min(0);
        self.living_base
            .add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::STRENGTH,
                time,
                amplifier,
            ));
        self.broadcast_entity_event(EntityStatus::ZombieConverting);
    }

    /// How much of the cure this tick completes.
    ///
    /// Vanilla parity: `ZombieVillager.getConversionProgress`. Once in a
    /// hundred ticks it counts iron bars and beds within four blocks, and each
    /// one has a thirty percent chance of shaving a tick off -- which is the
    /// whole reason a cure farm is a box of iron bars.
    fn conversion_progress(&self, world: &Arc<World>) -> i32 {
        let mut amount = 1;
        if rand::rng().random::<f32>() >= 0.01 {
            return amount;
        }

        let mut special_blocks = 0;
        let origin = self.position();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla casts the entity position to int the same way"
        )]
        let (x0, y0, z0) = (origin.x as i32, origin.y as i32, origin.z as i32);

        for x in (x0 - SPECIAL_BLOCK_RADIUS)..(x0 + SPECIAL_BLOCK_RADIUS) {
            for y in (y0 - SPECIAL_BLOCK_RADIUS)..(y0 + SPECIAL_BLOCK_RADIUS) {
                for z in (z0 - SPECIAL_BLOCK_RADIUS)..(z0 + SPECIAL_BLOCK_RADIUS) {
                    if special_blocks >= MAX_SPECIAL_BLOCKS {
                        return amount;
                    }
                    if !is_special_block(world, BlockPos::new(x, y, z)) {
                        continue;
                    }
                    if rand::rng().random::<f32>() < 0.3 {
                        amount += 1;
                    }
                    special_blocks += 1;
                }
            }
        }

        amount
    }

    /// Turns this zombie villager back into a villager.
    ///
    /// Vanilla parity: `ZombieVillager.finishConversion`. Everything it was
    /// carrying as a villager goes back: the profession, the level, the trades,
    /// the experience and, above all, the gossip -- and the player who fed it
    /// the apple is written into that gossip as a major positive, which is the
    /// discount that never wears off.
    fn finish_conversion(&self, world: &Arc<World>) {
        let villager_type = self.villager_type();
        let profession = self.profession();
        let level = self.villager_level();
        let finalized = self.villager_data_finalized();
        let gossips = self.gossips.lock().clone();
        let offers = self.trade_offers.lock().clone();
        let xp = self.villager_xp();
        let starter = *self.conversion_starter.lock();

        let converted = convert_to(
            self,
            // Vanilla parity: `ConversionParams.single(this, false, false)`.
            ConversionParams::single(false, false).with_reason(Cured),
            |id, position, world| {
                VillagerEntity::new(&vanilla_entities::VILLAGER, id, position, world)
            },
            |villager| {
                villager.set_villager_data_finalized(finalized);
                villager.set_villager_type(villager_type);
                villager.set_profession(profession);
                villager.set_level(level);
                if let Some(gossips) = &gossips {
                    villager.set_gossips(gossips);
                }
                if let Some(offers) = offers {
                    villager.merchant().set_offers(offers);
                }
                villager.merchant().set_xp(xp);
                villager
                    .living_base()
                    .add_mob_effect(MobEffectInstance::with_duration(
                        vanilla_mob_effects::NAUSEA,
                        CURE_NAUSEA_TICKS,
                        0,
                    ));
            },
        );

        let Some(villager) = converted else {
            return;
        };

        // Vanilla parity: `level.onReputationEvent(ZOMBIE_VILLAGER_CURED, ..)`,
        // which in vanilla reaches every villager nearby through the reputation
        // system. Foton has no such broadcast, so the cured villager records it
        // itself -- the one that matters, since it is the one being traded with.
        if let Some(starter) = starter {
            villager.on_reputation_event_from(ReputationEventType::ZombieVillagerCured, starter);
        }

        if !self.is_silent() {
            world.level_event(CURE_LEVEL_EVENT, self.block_position(), 0, None);
        }
    }
}

/// Vanilla parity: the `IRON_BARS`/`BedBlock` test of `getConversionProgress`.
fn is_special_block(world: &Arc<World>, pos: BlockPos) -> bool {
    use foton_registry::blocks::block_state_ext::BlockStateExt as _;
    let block = world.get_block_state(pos).get_block();
    block.key == vanilla_blocks::IRON_BARS.key || is_bed(block)
}

/// Vanilla parity: `state.getBlock() instanceof BedBlock`, which Foton has no
/// class hierarchy for; the `#minecraft:beds` tag names exactly the same blocks.
fn is_bed(block: BlockRef) -> bool {
    use foton_registry::TaggedRegistryExt as _;
    REGISTRY.blocks.is_in_tag(block, &BlockTag::BEDS)
}

/// Vanilla parity: `Difficulty.getId`.
const fn difficulty_id(difficulty: Difficulty) -> i32 {
    match difficulty {
        Difficulty::Peaceful => 0,
        Difficulty::Easy => 1,
        Difficulty::Normal => 2,
        Difficulty::Hard => 3,
    }
}

impl Entity for ZombieVillagerEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `ZombieVillager.tick`, which counts the cure down before
    /// it runs the zombie's own tick -- so a cure that completes this tick does
    /// so before anything else looks at the zombie.
    fn tick(&self) {
        self.tick_conversion();
        if self.is_removed() {
            return;
        }
        if let Some(living) = self.as_living_entity() {
            living.tick_living_entity();
        }
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if self.is_baby() {
            BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `ZombieVillager.addAdditionalSaveData`, on top of the
    /// zombie half it inherits.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        zombie_common::save_zombie(self, self.is_baby(), nbt);

        let mut villager_data = NbtCompound::new();
        villager_data.insert("type", self.villager_type().key.to_string());
        villager_data.insert("profession", self.profession().key.to_string());
        villager_data.insert("level", self.villager_level());
        nbt.insert("VillagerData", villager_data);
        nbt.insert("VillagerDataFinalized", self.villager_data_finalized());

        if let Some(offers) = self.trade_offers.lock().as_ref() {
            nbt.insert("Offers", offer_nbt::save(offers));
        }
        if let Some(gossips) = self.gossips.lock().as_ref() {
            nbt.insert("Gossips", gossips.save());
        }
        // Vanilla parity: `putInt("ConversionTime", isConverting() ? time : -1)`.
        nbt.insert(
            "ConversionTime",
            if self.is_converting() {
                self.conversion_time()
            } else {
                NOT_CONVERTING
            },
        );
        if let Some(starter) = *self.conversion_starter.lock() {
            nbt.insert(
                "ConversionPlayer",
                NbtTag::IntArray(starter.to_int_array().to_vec()),
            );
        }
        nbt.insert("Xp", self.villager_xp());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        zombie_common::load_zombie(self, nbt);

        if let Some(villager_data) = nbt.compound("VillagerData") {
            if let Some(key) = villager_data
                .string("type")
                .and_then(|value| Identifier::from_str(value.to_str().as_ref()).ok())
                && let Some(villager_type) = REGISTRY.villager_types.by_key(&key)
            {
                self.set_villager_type(villager_type);
            }
            if let Some(key) = villager_data
                .string("profession")
                .and_then(|value| Identifier::from_str(value.to_str().as_ref()).ok())
                && let Some(profession) = REGISTRY.villager_professions.by_key(&key)
            {
                self.set_profession(profession);
            }
            if let Some(level) = villager_data.int("level") {
                self.set_villager_level(level);
            }
            self.set_villager_data_finalized(true);
        }
        if nbt
            .byte("VillagerDataFinalized")
            .is_some_and(|value| value != 0)
        {
            self.set_villager_data_finalized(true);
        }

        if let Some(list) = nbt.list("Offers") {
            *self.trade_offers.lock() = Some(offer_nbt::load(&list));
        }
        if let Some(list) = nbt.list("Gossips") {
            let mut gossips = GossipContainer::new();
            gossips.load(&list);
            *self.gossips.lock() = Some(gossips);
        }
        if let Some(xp) = nbt.int("Xp") {
            self.set_villager_xp(xp);
        }

        let conversion_time = nbt.int("ConversionTime").unwrap_or(NOT_CONVERTING);
        if conversion_time == NOT_CONVERTING {
            self.entity_data.lock().converting.set(false);
            *self.villager_conversion_time.lock() = NOT_CONVERTING;
            return;
        }
        let starter = nbt
            .int_array("ConversionPlayer")
            .and_then(|array| Uuid::from_int_array(&array));
        self.start_converting(starter, conversion_time);
    }
}

impl LivingEntity for ZombieVillagerEntity {
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

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_VILLAGER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_VILLAGER_DEATH)
    }

    /// Vanilla parity: the `VILLAGER_VARIANT` a zombie villager answers, which
    /// is what lets a trade's `merchant_predicate` read it after a cure.
    fn villager_loot_variant(&self) -> Option<&'static Identifier> {
        Some(&self.villager_type().key)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.default_ai_step()
    }
}

impl Mob for ZombieVillagerEntity {
    /// Vanilla parity: `Mob.serverAiStep` ticks the goal selector for every
    /// mob it runs, brain-driven or not. `Mob::tick_goal_selectors` has an
    /// empty default, so leaving it out is how a registered goal set never
    /// runs.
    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    /// Sets whether this is a baby zombie villager.
    ///
    /// Vanilla parity: the `Zombie.setBaby` a zombie villager inherits. The
    /// hitbox follows because [`Self::dimensions_for_pose`] reads the flag.
    fn set_baby(&self, baby: bool) {
        self.entity_data.lock().zombie.baby.set(baby);
        self.refresh_dimensions();
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().zombie.mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .zombie
            .mob_mut()
            .mob_flags
            .set(flags);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_VILLAGER_AMBIENT)
    }

    /// Vanilla parity: `ZombieVillager.removeWhenFarAway`, which keeps a zombie
    /// villager alive while it is being cured or while it still remembers a
    /// trade -- so a cure in progress is never despawned out from under a
    /// player who walked away.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !self.is_converting() && self.villager_xp() == 0
    }

    /// Vanilla parity: `ZombieVillager.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let stack = player.inventory.lock().get_item_in_hand(hand).clone();
        if !stack.is(&vanilla_items::GOLDEN_APPLE) {
            return InteractionResult::Pass;
        }
        if !self
            .living_base
            .has_mob_effect(vanilla_mob_effects::WEAKNESS)
        {
            // Vanilla parity: a golden apple without weakness is refused, not
            // eaten -- which is why the potion has to come first.
            return InteractionResult::Consume;
        }

        self.use_player_item(player, hand);
        let time = CONVERSION_WAIT_MIN + rand::rng().random_range(0..CONVERSION_WAIT_SPREAD);
        self.start_converting(Some(player.uuid()), time);
        InteractionResult::Success
    }
}

impl PathfinderMob for ZombieVillagerEntity {}

impl Enemy for ZombieVillagerEntity {}

impl ZombieVillagerEntity {
    /// Runs the cure down by this tick's worth of progress.
    ///
    /// Vanilla parity: the top of `ZombieVillager.tick`, which counts down
    /// before `super.tick()` so a cure that completes does so before the
    /// zombie's own tick runs.
    pub fn tick_conversion(&self) {
        if !LivingEntity::is_alive(self) || !self.is_converting() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };

        let amount = self.conversion_progress(&world);
        let remaining = {
            let mut time = self.villager_conversion_time.lock();
            *time -= amount;
            *time
        };
        if remaining <= 0 {
            self.finish_conversion(&world);
        }
    }
}
