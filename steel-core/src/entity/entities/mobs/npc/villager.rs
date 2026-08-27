//! The villager.
//!
//! Vanilla parity: `net.minecraft.world.entity.npc.villager.Villager` and the
//! half of `AbstractVillager` that is not the `Merchant` seam (which lives in
//! [`super::merchant_state`]).
//!
//! What is here is the trading loop end to end -- a profession and a level, the
//! trades that follow from them, the screen a player buys through, restocking,
//! leveling up, the reputation that discounts a price -- and the seam onto the
//! brain that runs the villager's day. The day itself is in
//! [`super::villager_ai`].

use std::borrow::Cow;
use std::str::FromStr as _;
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::item_stack::ItemStack;
use steel_registry::loot_table::{EntityRef, LootContext};
use steel_registry::poi::PoiTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::trading::{MerchantOffers, TradeSet, offer_nbt};
use steel_registry::vanilla_entity_data::VillagerEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::vanilla_poi_type_tags::PoiTag;
use steel_registry::villager_profession::VillagerProfessionRef;
use steel_registry::villager_type::VillagerTypeRef;
use steel_registry::{
    REGISTRY, RegistryEntry as _, RegistryExt as _, TaggedRegistryExt as _, sound_events,
    vanilla_items, vanilla_mob_effects, vanilla_poi_types, vanilla_villager_professions,
    vanilla_villager_types,
};
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{DowncastType, DowncastTypeKey, GlobalPos, Identifier};
use text_components::TextComponent;
use text_components::translation::TranslatedMessage;
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::memory::{MemoryModuleType, memory_module_types};
use crate::entity::ai::brain::{Brain, ScheduleAttribute};
use crate::entity::ai::gossip::{GossipContainer, GossipType, ReputationEventType};
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::npc::merchant_state::{MerchantState, villager_data};
use crate::entity::entities::mobs::npc::villager_ai;
use crate::entity::inventory_carrier::{self, InventoryCarrier, load_inventory, save_inventory};
use crate::entity::{
    AgeableMob, AgeableMobBase, Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob, SharedEntity,
};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::trading::{Merchant, open_trading_screen};
use crate::world::World;

/// Vanilla parity: `Villager.BABY_DIMENSIONS`, whose numbers are also the
/// `baby_dimensions` of the extracted `entities.json` entry for `villager`.
const BABY_WIDTH: f32 = 0.49;
const BABY_HEIGHT: f32 = 0.98;
const BABY_EYE_HEIGHT: f32 = 0.63;
const BABY_DIMENSIONS: EntityDimensions =
    EntityDimensions::new(BABY_WIDTH, BABY_HEIGHT, BABY_EYE_HEIGHT);

/// Vanilla parity: `Villager.BREEDING_FOOD_THRESHOLD`.
const BREEDING_FOOD_THRESHOLD: i32 = 12;
/// Vanilla parity: `Villager.GOSSIP_DECAY_INTERVAL`.
const GOSSIP_DECAY_INTERVAL: i64 = 24_000;
/// Vanilla parity: `Villager.MAX_GOSSIP_TOPICS`.
const MAX_GOSSIP_TOPICS: i32 = 10;
/// Vanilla parity: `Villager.GOSSIP_COOLDOWN`.
const GOSSIP_COOLDOWN: i64 = 1_200;
/// Vanilla parity: the `40` a villager's unhappy counter starts at.
const UNHAPPY_TICKS: i32 = 40;
/// Vanilla parity: the `200` ticks of regeneration a level-up grants.
const LEVEL_UP_REGENERATION_TICKS: i32 = 200;
/// Vanilla parity: the half day `Villager.shouldRestock` measures against.
const HALF_DAY_TICKS: i64 = 12_000;
/// Vanilla parity: the `2400` between a day's two allowed restocks.
const RESTOCK_COOLDOWN_TICKS: i64 = 2_400;
/// Vanilla parity: the `2` restocks a villager is allowed each day.
const MAX_RESTOCKS_PER_DAY: i32 = 2;
/// Vanilla parity: the `countItem(Items.BREAD) <= 36` of `makeBread`.
const MAX_BREAD_BEFORE_BAKING: i32 = 36;
/// Vanilla parity: `makeBread`'s `maxAmountOfBreadToMake`.
const MAX_BREAD_PER_BAKE: i32 = 3;
/// Vanilla parity: `makeBread`'s `amountOfWheatNeededToCraftOneBread`.
const WHEAT_PER_BREAD: i32 = 3;
/// Vanilla parity: the `0.5F` offset the bread it cannot carry is dropped at.
const BREAD_DROP_OFFSET: f64 = 0.5;

/// Vanilla parity: `Villager.FOOD_POINTS`.
fn food_points(stack: &ItemStack) -> i32 {
    let key = &stack.item().key;
    if key.namespace != "minecraft" {
        return 0;
    }
    match key.path.as_ref() {
        "bread" => 4,
        "potato" | "carrot" | "beetroot" => 1,
        _ => 0,
    }
}

/// A villager: a mob that trades, restocks and remembers.
#[entity_behavior(class = "Villager")]
pub struct VillagerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    entity_data: SyncMutex<VillagerEntityData>,
    /// The trading state, in an `Arc` so the screen can hold it.
    merchant: Arc<MerchantState>,
    /// Vanilla parity: `AbstractVillager.inventory`, a `SimpleContainer(8)`.
    inventory: SyncMutex<SimpleContainer>,
    gossips: SyncMutex<GossipContainer>,
    /// Vanilla parity: `LivingEntity.brain`, built by `Villager.BRAIN_PROVIDER`.
    brain: Brain,
    food_level: SyncMutex<i32>,
    last_gossip_time: SyncMutex<i64>,
    last_gossip_decay_time: SyncMutex<i64>,
    last_restock_game_time: SyncMutex<i64>,
    number_of_restocks_today: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `VillagerEntity`.
unsafe impl DowncastType for VillagerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/villager");
}

impl VillagerEntity {
    /// Vanilla parity: `AbstractVillager.inventory`'s eight slots.
    const INVENTORY_SIZE: usize = 8;

    /// Creates a new villager at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world.clone()),
            entity_type,
            id,
            world,
        )
    }

    /// Reconstructs a villager from persisted base entity state.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        let id = load.id;
        let world = load.world.clone();
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            id,
            world,
        )
    }

    fn new_with_base(
        base: EntityBase,
        entity_type: EntityTypeRef,
        id: i32,
        world: Weak<World>,
    ) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        let ageable_base = AgeableMobBase::new();
        let mut entity_data = VillagerEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let villager = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            entity_data: SyncMutex::new(entity_data),
            merchant: Arc::new(MerchantState::villager(id, world)),
            inventory: SyncMutex::new(SimpleContainer::new(Self::INVENTORY_SIZE)),
            gossips: SyncMutex::new(GossipContainer::new()),
            brain: villager_ai::make_brain(),
            food_level: SyncMutex::new(0),
            last_gossip_time: SyncMutex::new(0),
            last_gossip_decay_time: SyncMutex::new(0),
            last_restock_game_time: SyncMutex::new(0),
            number_of_restocks_today: SyncMutex::new(0),
        };
        villager.update_schedule();
        // Vanilla parity: the `setCanPickUpLoot(true)` of the `Villager`
        // constructor, which is what makes `Mob.aiStep` offer the ground to
        // `wantsToPickUp` at all.
        Mob::set_can_pick_up_loot(&villager, true);
        villager
    }

    /// Points the brain at the schedule track for this villager's age.
    ///
    /// Vanilla parity: the `brain.setSchedule(...)` half of
    /// `Villager.registerBrainGoals`. Vanilla picks the attribute while it
    /// rebuilds the brain; Steel's brain outlives growing up, so this is called
    /// again whenever the baby flag changes or is loaded.
    fn update_schedule(&self) {
        self.brain.set_schedule(if AgeableMob::is_baby(self) {
            ScheduleAttribute::BabyVillagerActivity
        } else {
            ScheduleAttribute::VillagerActivity
        });
    }

    /// The trading state, for a caller that needs the `Merchant` seam.
    #[must_use]
    pub const fn merchant(&self) -> &Arc<MerchantState> {
        &self.merchant
    }

    /// This villager's biome variant.
    ///
    /// Vanilla parity: `VillagerData.type`.
    #[must_use]
    pub fn villager_type(&self) -> VillagerTypeRef {
        let id = self.entity_data.lock().villager_data.get().villager_type;
        usize::try_from(id)
            .ok()
            .and_then(|id| REGISTRY.villager_types.by_id(id))
            .unwrap_or(&vanilla_villager_types::PLAINS)
    }

    /// This villager's profession.
    ///
    /// Vanilla parity: `VillagerData.profession`.
    #[must_use]
    pub fn profession(&self) -> VillagerProfessionRef {
        let id = self.entity_data.lock().villager_data.get().profession;
        usize::try_from(id)
            .ok()
            .and_then(|id| REGISTRY.villager_professions.by_id(id))
            .unwrap_or(&vanilla_villager_professions::NONE)
    }

    /// This villager's trading level, 1..=5.
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

    /// Sets the biome variant, which decides which biome-gated trades appear.
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
    /// Vanilla parity: `Villager.setVillagerData`, whose first act is to drop
    /// the offer list when the profession actually changed.
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
            self.merchant.clear_offers();
        }
    }

    /// Sets the trading level and mirrors it onto the trading screen.
    pub fn set_level(&self, level: i32) {
        let level = level.clamp(villager_data::MIN_LEVEL, villager_data::MAX_LEVEL);
        {
            let mut data = self.entity_data.lock();
            let mut villager_data = *data.villager_data.get();
            villager_data.level = level;
            data.villager_data.set(villager_data);
        }
        self.merchant.set_level(level);
    }

    /// Vanilla parity: `AbstractVillager.getUnhappyCounter`.
    #[must_use]
    pub fn unhappy_counter(&self) -> i32 {
        *self
            .entity_data
            .lock()
            .abstract_villager
            .unhappy_counter
            .get()
    }

    /// Vanilla parity: `AbstractVillager.setUnhappyCounter`.
    pub fn set_unhappy_counter(&self, counter: i32) {
        self.entity_data
            .lock()
            .abstract_villager
            .unhappy_counter
            .set(counter);
    }

    /// The offers this villager is currently willing to make, rolling them the
    /// first time anything asks.
    ///
    /// Vanilla parity: `AbstractVillager.getOffers`, which lazily builds the
    /// list and then calls `updateTrades`.
    pub fn offers(&self) -> MerchantOffers {
        if !self.merchant.offers_built() {
            self.update_trades();
            self.merchant.mark_offers_built();
        }
        self.merchant.offers().lock().clone()
    }

    /// Rolls the trades this profession offers at this level and adds them.
    ///
    /// Vanilla parity: `Villager.updateTrades`, which looks up the trade set
    /// for the current level and appends what it draws to the existing list --
    /// so leveling up adds trades rather than replacing them.
    pub fn update_trades(&self) {
        let profession = self.profession();
        let Some(trade_set) = TradeSet::for_profession(&profession.key, self.villager_level())
        else {
            return;
        };

        let mut rng = rand::rng();
        let villager_variant = self.villager_type().key.clone();
        let position = self.position();
        let mut context = LootContext::new(&mut rng)
            .with_origin(position.x, position.y, position.z)
            .allowing_additional_cost_component()
            .with_this_entity(EntityRef {
                entity_type: Some(&self.entity_type.key),
                villager_variant: Some(&villager_variant),
                ..EntityRef::default()
            });

        let mut offers = self.merchant.offers().lock();
        trade_set.add_offers(&mut context, &mut offers);
    }

    /// Whether this villager is showing a player its screen right now.
    ///
    /// Vanilla parity: `AbstractVillager.isTrading`.
    #[must_use]
    pub fn is_trading(&self) -> bool {
        self.merchant.trading_player().is_some()
    }

    /// Vanilla parity: `Villager.getPlayerReputation`.
    #[must_use]
    pub fn player_reputation(&self, player: Uuid) -> i32 {
        self.gossips.lock().reputation(player, |_| true)
    }

    /// Vanilla parity: `Villager.getGossips`.
    #[must_use]
    pub fn gossips(&self) -> GossipContainer {
        self.gossips.lock().copy()
    }

    /// Vanilla parity: `Villager.setGossips`, which merges rather than replaces.
    pub fn set_gossips(&self, gossips: &GossipContainer) {
        self.gossips.lock().put_all(gossips);
    }

    /// Records something that happened to this villager.
    ///
    /// Vanilla parity: `Villager.onReputationEventFrom`.
    pub fn on_reputation_event_from(&self, event: ReputationEventType, source: Uuid) {
        let mut gossips = self.gossips.lock();
        match event {
            ReputationEventType::ZombieVillagerCured => {
                gossips.add(
                    source,
                    GossipType::MajorPositive,
                    GossipType::REPUTATION_CHANGE_PER_EVERLASTING_MEMORY,
                );
                gossips.add(
                    source,
                    GossipType::MinorPositive,
                    GossipType::REPUTATION_CHANGE_PER_EVENT,
                );
            }
            ReputationEventType::Trade => {
                gossips.add(
                    source,
                    GossipType::Trading,
                    GossipType::REPUTATION_CHANGE_PER_TRADE,
                );
            }
            ReputationEventType::VillagerHurt => {
                gossips.add(
                    source,
                    GossipType::MinorNegative,
                    GossipType::REPUTATION_CHANGE_PER_EVENT,
                );
            }
            ReputationEventType::VillagerKilled => {
                gossips.add(
                    source,
                    GossipType::MajorNegative,
                    GossipType::REPUTATION_CHANGE_PER_EVENT,
                );
            }
            // Vanilla's `Villager.onReputationEventFrom` has no arm for a golem
            // being killed; only the iron golem's own handler reacts to that.
            ReputationEventType::GolemKilled => {}
        }
    }

    /// Swaps gossip with another villager standing nearby.
    ///
    /// Vanilla parity: `Villager.gossip`, gated on both villagers being off
    /// their thousand-two-hundred-tick cooldown.
    pub fn gossip_with(&self, other: &Self, timestamp: i64) {
        let mine = *self.last_gossip_time.lock();
        let theirs = *other.last_gossip_time.lock();
        let ready = |last: i64| timestamp < last || timestamp >= last + GOSSIP_COOLDOWN;
        if !ready(mine) || !ready(theirs) {
            return;
        }

        let source = other.gossips.lock().copy();
        let mut rng = rand::rng();
        self.gossips
            .lock()
            .transfer_from(&source, &mut rng, MAX_GOSSIP_TOPICS);
        *self.last_gossip_time.lock() = timestamp;
        *other.last_gossip_time.lock() = timestamp;
    }

    /// Vanilla parity: `Villager.maybeDecayGossip`.
    fn maybe_decay_gossip(&self, game_time: i64) {
        let mut last = self.last_gossip_decay_time.lock();
        if *last == 0 {
            *last = game_time;
            return;
        }
        if game_time < *last + GOSSIP_DECAY_INTERVAL {
            return;
        }
        self.gossips.lock().decay();
        *last = game_time;
    }

    /// Whether this villager may restock right now.
    ///
    /// Vanilla parity: `Villager.shouldRestock` and the `allowedToRestock` it
    /// guards. Note that the day rollover is what resets the counter, and that
    /// resetting it also catches the demand up on the restocks that were missed.
    #[must_use]
    pub fn should_restock(&self, game_time: i64) -> bool {
        let is_new_day = {
            let last = *self.last_restock_game_time.lock();
            game_time > last + HALF_DAY_TICKS
        };
        if is_new_day {
            *self.last_restock_game_time.lock() = game_time;
            self.reset_number_of_restocks();
        }
        self.allowed_to_restock(game_time) && self.merchant.needs_to_restock()
    }

    fn allowed_to_restock(&self, game_time: i64) -> bool {
        let restocks = *self.number_of_restocks_today.lock();
        restocks == 0
            || (restocks < MAX_RESTOCKS_PER_DAY
                && game_time > *self.last_restock_game_time.lock() + RESTOCK_COOLDOWN_TICKS)
    }

    /// Puts every trade back in stock and folds its use count into demand.
    ///
    /// Vanilla parity: `Villager.restock`.
    pub fn restock(&self, game_time: i64) {
        self.merchant.update_demand();
        self.merchant.reset_uses();
        *self.last_restock_game_time.lock() = game_time;
        *self.number_of_restocks_today.lock() += 1;
    }

    /// Vanilla parity: `Villager.resetNumberOfRestocks`, which catches demand
    /// up on whatever restocks the day did not use before clearing the count.
    fn reset_number_of_restocks(&self) {
        let missed = MAX_RESTOCKS_PER_DAY - *self.number_of_restocks_today.lock();
        if missed > 0 {
            self.merchant.reset_uses();
        }
        for _ in 0..missed {
            self.merchant.update_demand();
        }
        *self.number_of_restocks_today.lock() = 0;
    }

    /// Raises the trading level by one and rolls the trades it unlocks.
    ///
    /// Vanilla parity: `Villager.increaseMerchantCareer`.
    fn increase_merchant_career(&self) {
        self.set_level(self.villager_level() + 1);
        self.update_trades();
    }

    /// Vanilla parity: `Villager.setUnhappy`.
    fn set_unhappy(&self) {
        self.set_unhappy_counter(UNHAPPY_TICKS);
        self.play_sound(&sound_events::ENTITY_VILLAGER_NO, 1.0, 1.0);
    }

    /// Opens the trading screen for `player`.
    ///
    /// Vanilla parity: `Villager.startTrading`.
    fn start_trading(&self, player: &Player) {
        self.merchant
            .update_special_prices(player, self.player_reputation(player.uuid()));
        self.merchant.set_trading_player(Some(player.uuid()));
        let merchant: Arc<dyn Merchant> = Arc::clone(&self.merchant) as _;
        open_trading_screen(&merchant, player, self.profession_name());
    }

    /// The name the trading screen is titled with.
    ///
    /// Vanilla parity: `Villager.getTypeName`, which is the profession's own
    /// translated name rather than the entity type's.
    fn profession_name(&self) -> TextComponent {
        let profession = self.profession();
        TextComponent::translated(TranslatedMessage {
            key: Cow::Owned(format!(
                "entity.{}.villager.{}",
                profession.key.namespace, profession.key.path
            )),
            fallback: None,
            args: None,
        })
    }

    /// Vanilla parity: `Villager.stopTrading`, which also clears the reputation
    /// discount so it is recomputed for whoever opens the screen next.
    pub fn stop_trading(&self) {
        self.merchant.set_trading_player(None);
        self.merchant.reset_special_prices();
    }

    /// Vanilla parity: `Villager.countFoodPointsInInventory`.
    #[must_use]
    pub fn food_points_in_inventory(&self) -> i32 {
        self.inventory
            .lock()
            .items()
            .iter()
            .map(|stack| food_points(stack) * stack.count())
            .sum()
    }

    /// Vanilla parity: `Villager.canBreed`.
    #[must_use]
    pub fn can_breed(&self) -> bool {
        *self.food_level.lock() + self.food_points_in_inventory() >= BREEDING_FOOD_THRESHOLD
            && AgeableMob::get_age(self) == 0
    }

    /// Vanilla parity: `Villager.eatAndDigestFood`.
    pub fn eat_and_digest_food(&self) {
        self.eat_until_full();
        *self.food_level.lock() -= BREEDING_FOOD_THRESHOLD;
    }

    /// Vanilla parity: `Villager.eatUntilFull`.
    fn eat_until_full(&self) {
        if *self.food_level.lock() >= BREEDING_FOOD_THRESHOLD {
            return;
        }
        let mut inventory = self.inventory.lock();
        for slot in 0..inventory.get_container_size() {
            let value = food_points(inventory.get_item(slot));
            if value == 0 {
                continue;
            }
            while inventory.get_item(slot).count() > 0 {
                *self.food_level.lock() += value;
                inventory.remove_item(slot, 1);
                if *self.food_level.lock() >= BREEDING_FOOD_THRESHOLD {
                    return;
                }
            }
        }
    }

    /// Vanilla parity: `Villager.hasExcessFood`.
    #[must_use]
    pub fn has_excess_food(&self) -> bool {
        self.food_points_in_inventory() >= BREEDING_FOOD_THRESHOLD * 2
    }

    /// Vanilla parity: `Villager.wantsMoreFood`.
    #[must_use]
    pub fn wants_more_food(&self) -> bool {
        self.food_points_in_inventory() < BREEDING_FOOD_THRESHOLD
    }

    /// Bakes bread out of the wheat this villager is carrying.
    ///
    /// Vanilla parity: `WorkAtComposter.makeBread`, which is the only thing that
    /// turns a farmer's harvest into food a village can eat and breed on. Bread
    /// it cannot carry is dropped rather than lost.
    pub fn make_bread(&self) {
        let leftover = {
            let mut inventory = self.inventory.lock();
            if inventory.count_item(&vanilla_items::BREAD) > MAX_BREAD_BEFORE_BAKING {
                return;
            }
            let wheat = inventory.count_item(&vanilla_items::WHEAT);
            let loaves = MAX_BREAD_PER_BAKE.min(wheat / WHEAT_PER_BREAD);
            if loaves == 0 {
                return;
            }
            inventory.remove_item_type(&vanilla_items::WHEAT, loaves * WHEAT_PER_BREAD);
            let mut bread = ItemStack::with_count(&vanilla_items::BREAD, loaves);
            inventory.add(&mut bread);
            bread
        };
        if !leftover.is_empty() {
            self.spawn_at_location(leftover, BREAD_DROP_OFFSET);
        }
    }

    /// Vanilla parity: `Villager.hasFarmSeeds`.
    #[must_use]
    pub fn has_farm_seeds(&self) -> bool {
        self.inventory
            .lock()
            .items()
            .iter()
            .any(|stack| stack.item().has_tag(&ItemTag::VILLAGER_PLANTABLE_SEEDS))
    }

    /// The experience this villager has banked toward its next level.
    ///
    /// Vanilla parity: `Villager.getVillagerXp`.
    #[must_use]
    pub fn villager_xp(&self) -> i32 {
        self.merchant.xp()
    }

    /// Vanilla parity: `Villager.playWorkSound`.
    pub fn play_work_sound(&self) {
        self.make_sound(self.profession().work_sound);
    }

    /// Gives back the POI ticket a memory holds.
    ///
    /// Vanilla parity: `Villager.releasePoi`, which only releases when the block
    /// is still a POI this villager's memory is allowed to hold -- so a job site
    /// that has since become something else is left alone.
    pub fn release_poi(&self, memory: MemoryModuleType<GlobalPos>) {
        let Some(world) = Entity::level(self) else {
            return;
        };
        let Some(held) = self.brain.get_memory(memory) else {
            return;
        };
        if held.dimension != world.key {
            return;
        }
        let mut storage = world.poi_storage.lock();
        let Some(poi_type) = storage
            .get_type(held.pos)
            .and_then(|id| REGISTRY.poi_types.by_id(id))
        else {
            return;
        };
        if self.poi_memory_accepts(memory, poi_type) {
            let _released = storage.release_ticket(held.pos);
        }
    }

    /// Vanilla parity: the `POI_MEMORIES` map, which pairs each POI memory with
    /// the predicate saying whether this villager may hold that kind of POI in
    /// it.
    fn poi_memory_accepts(
        &self,
        memory: MemoryModuleType<GlobalPos>,
        poi_type: PoiTypeRef,
    ) -> bool {
        let profession = self.profession();
        let jobless = profession.key.path == "none" || profession.key.path == "nitwit";
        match memory.id().key() {
            "minecraft:home" => poi_type.key == vanilla_poi_types::HOME.key,
            "minecraft:meeting_point" => poi_type.key == vanilla_poi_types::MEETING.key,
            "minecraft:job_site" => !jobless && poi_type.key == profession.key,
            "minecraft:potential_job_site" => REGISTRY
                .poi_types
                .is_in_tag(poi_type, &PoiTag::ACQUIRABLE_JOB_SITE),
            _ => false,
        }
    }

    /// Runs the once-a-tick trading bookkeeping.
    ///
    /// Vanilla parity: the `updateMerchantTimer` and `lastTradedPlayer` halves
    /// of `Villager.customServerAiStep`.
    fn tick_merchant(&self, world: &Arc<World>) {
        if !self.is_trading()
            && let Some(level_up) = self.merchant.tick_level_up_timer()
        {
            if level_up {
                self.increase_merchant_career();
            }
            self.living_base
                .add_mob_effect(MobEffectInstance::with_duration(
                    vanilla_mob_effects::REGENERATION,
                    LEVEL_UP_REGENERATION_TICKS,
                    0,
                ));
        }

        if let Some(player) = self.merchant.take_last_traded_player() {
            self.on_reputation_event_from(ReputationEventType::Trade, player);
            self.broadcast_entity_event(EntityStatus::VillagerHappy);
        }

        // Vanilla parity: a villager whose profession was taken away mid-trade
        // has its screen closed.
        if self.profession().key.path == "none" && self.is_trading() {
            self.stop_trading();
        }

        let game_time = world.game_time();
        if self.should_restock(game_time) {
            self.restock(game_time);
        }
    }
}

impl Entity for VillagerEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);

        if self.unhappy_counter() > 0 {
            self.set_unhappy_counter(self.unhappy_counter() - 1);
        }
        if let Some(world) = Entity::level(self) {
            self.maybe_decay_gossip(world.game_time());
        }
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
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
        SoundSource::Neutral
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);

        let mut villager_data = NbtCompound::new();
        villager_data.insert("type", self.villager_type().key.to_string());
        villager_data.insert("profession", self.profession().key.to_string());
        villager_data.insert("level", self.villager_level());
        nbt.insert("VillagerData", villager_data);
        nbt.insert("VillagerDataFinalized", self.villager_data_finalized());

        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla stores the food level as a byte"
        )]
        nbt.insert("FoodLevel", *self.food_level.lock() as i8);
        nbt.insert("Gossips", self.gossips.lock().save());
        nbt.insert("Xp", self.merchant.xp());
        nbt.insert("LastRestock", *self.last_restock_game_time.lock());
        nbt.insert("LastGossipDecay", *self.last_gossip_decay_time.lock());
        nbt.insert("RestocksToday", *self.number_of_restocks_today.lock());

        if self.merchant.offers_built() {
            nbt.insert("Offers", offer_nbt::save(&self.merchant.offers().lock()));
        }
        save_inventory(&self.inventory.lock(), nbt);
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);

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
                self.set_level(level);
            }
            self.set_villager_data_finalized(true);
        }
        if nbt
            .byte("VillagerDataFinalized")
            .is_some_and(|value| value != 0)
        {
            self.set_villager_data_finalized(true);
        }

        if let Some(food_level) = nbt.byte("FoodLevel") {
            *self.food_level.lock() = i32::from(food_level);
        }
        if let Some(list) = nbt.list("Gossips") {
            self.gossips.lock().load(&list);
        }
        if let Some(xp) = nbt.int("Xp") {
            self.merchant.set_xp(xp);
        }
        if let Some(last_restock) = nbt.long("LastRestock") {
            *self.last_restock_game_time.lock() = last_restock;
        }
        if let Some(last_decay) = nbt.long("LastGossipDecay") {
            *self.last_gossip_decay_time.lock() = last_decay;
        }
        if let Some(restocks) = nbt.int("RestocksToday") {
            *self.number_of_restocks_today.lock() = restocks;
        }
        if let Some(list) = nbt.list("Offers") {
            self.merchant.set_offers(offer_nbt::load(&list));
        }
        load_inventory(&mut self.inventory.lock(), nbt);
        self.brain.load(nbt);
        // The age came out of the same NBT, so which schedule track this
        // villager reads may have changed with it.
        self.update_schedule();
    }
}

impl VillagerEntity {
    /// Gives up the workstation, bed and meeting point this villager held.
    ///
    /// Vanilla parity: `Villager.releaseAllPois`, called from `die` and from
    /// every conversion.
    pub fn release_all_pois(&self) {
        for memory in [
            memory_module_types::HOME,
            memory_module_types::JOB_SITE,
            memory_module_types::POTENTIAL_JOB_SITE,
            memory_module_types::MEETING_POINT,
        ] {
            self.release_poi(memory);
        }
    }
}

impl LivingEntity for VillagerEntity {
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
        Some(&sound_events::ENTITY_VILLAGER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_VILLAGER_DEATH)
    }

    /// Vanilla parity: `Villager.villagerLootVariant`, by way of the
    /// `VILLAGER_VARIANT` implicit component a villager answers.
    fn villager_loot_variant(&self) -> Option<&'static Identifier> {
        Some(&self.villager_type().key)
    }

    /// Vanilla parity: `Villager.die`, which lets go of the workstation and bed
    /// before the death runs -- a villager killed at its bench must not leave
    /// the bench claimed forever.
    fn die(&self, source: &DamageSource) {
        self.release_all_pois();
        self.living_die(source);
    }

    /// Vanilla parity: `Villager.customServerAiStep`, which ticks the brain
    /// before the trading bookkeeping.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
        let Some(world) = Entity::level(self) else {
            return;
        };
        self.brain.tick(&world, self);
        self.tick_merchant(&world);
    }

    /// Vanilla parity: the `Villager.stopSleeping` override, the only writer of
    /// `LAST_WOKEN` -- which is what stops `SleepInBed` putting a villager that
    /// was just shaken awake straight back into the bed.
    fn stop_sleeping(&self) {
        self.default_stop_sleeping();
        if let Some(world) = Entity::level(self) {
            self.brain
                .set_memory(memory_module_types::LAST_WOKEN, world.game_time());
        }
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        result
    }
}

impl AgeableMob for VillagerEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self
            .entity_data
            .lock()
            .abstract_villager
            .ageable_mob()
            .age_locked
            .get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .abstract_villager
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data
            .lock()
            .abstract_villager
            .ageable_mob_mut()
            .baby
            .set(baby);
    }

    /// Vanilla parity: `Villager.ageBoundaryReached`, which rebuilds the brain
    /// so a grown villager reads the adult schedule and gets the WORK package.
    /// Steel registers both packages on every villager and only has to swap the
    /// schedule track -- see the module docs on [`villager_ai`].
    fn age_boundary_changed(&self, _baby: bool) {
        self.refresh_dimensions();
        self.update_schedule();
    }
}

impl Mob for VillagerEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn mob_flags(&self) -> i8 {
        *self
            .entity_data
            .lock()
            .abstract_villager
            .mob()
            .mob_flags
            .get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .abstract_villager
            .mob_mut()
            .mob_flags
            .set(flags);
    }

    /// Vanilla parity: `Villager.removeWhenFarAway`, which is always false --
    /// a villager is never despawned for being alone.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    /// Vanilla parity: `Villager.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        if LivingEntity::is_sleeping(self) {
            return None;
        }
        Some(if self.is_trading() {
            &sound_events::ENTITY_VILLAGER_TRADE
        } else {
            &sound_events::ENTITY_VILLAGER_AMBIENT
        })
    }

    /// Vanilla parity: `Villager.wantsToPickUp`.
    ///
    /// This deliberately does not defer to [`Mob::mob_wants_to_pick_up`]: the
    /// vanilla override replaces the shared body rather than narrowing it, so a
    /// villager never asks `canHoldItem` -- the question is whether the stack is
    /// one it collects and whether its own container has room.
    /// [`Mob::pick_up_nearby_items`] has already checked `canPickUpLoot` and
    /// `mobGriefing`, exactly where `Mob.aiStep` checks them.
    fn wants_to_pick_up(&self, _world: &World, item_stack: &ItemStack) -> bool {
        (item_stack.item().has_tag(&ItemTag::VILLAGER_PICKS_UP)
            || self.profession().requests_item(item_stack.item()))
            && inventory_carrier::can_add_item(&self.inventory.lock(), item_stack)
    }

    /// Vanilla parity: `Villager.pickUpItem`, which stows the stack in the
    /// villager's own container instead of equipping it.
    fn pick_up_item(&self, world: &Arc<World>, item_entity: &SharedEntity) {
        inventory_carrier::pick_up_item(world, self, item_entity);
    }

    /// Vanilla parity: `Villager.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let stack = player.inventory.lock().get_item_in_hand(hand).clone();
        if stack.item().key.path == "villager_spawn_egg"
            || !LivingEntity::is_alive(self)
            || self.is_trading()
            || LivingEntity::is_sleeping(self)
        {
            return InteractionResult::Pass;
        }

        if AgeableMob::is_baby(self) {
            self.set_unhappy();
            return InteractionResult::Success;
        }

        let offers = self.offers();
        let no_offers = offers.is_empty();
        if hand == InteractionHand::MainHand && no_offers {
            self.set_unhappy();
        }
        if no_offers {
            return InteractionResult::Consume;
        }

        self.start_trading(player);
        InteractionResult::Success
    }
}

impl PathfinderMob for VillagerEntity {}

impl InventoryCarrier for VillagerEntity {
    fn carried_inventory(&self) -> &SyncMutex<SimpleContainer> {
        &self.inventory
    }
}
