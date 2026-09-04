//! The wandering trader.
//!
//! Vanilla parity: `net.minecraft.world.entity.npc.wanderingtrader.WanderingTrader`.
//!
//! It trades like a villager and nothing else about it is the same: it never
//! levels up, never restocks, shows no experience bar, and leaves after its
//! despawn delay runs out. Its stock is drawn once from three pools -- the
//! things it buys, an uncommon and a common set -- and that is all it will ever
//! sell.

use std::borrow::Cow;
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::{EntityDimensions, EntityTypeRef};
use foton_registry::loot_table::{EntityRef, LootContext};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::trading::{MerchantOffers, offer_nbt};
use foton_registry::vanilla_entities;
use foton_registry::vanilla_entity_data::WanderingTraderEntityData;
use foton_registry::{REGISTRY, RegistryExt as _, sound_events};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, DowncastType, DowncastTypeKey, Identifier};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use text_components::TextComponent;
use text_components::translation::TranslatedMessage;

use crate::behavior::InteractionResult;
use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::goal::{
    AvoidEntityGoal, FloatGoal, InteractGoal, LookAtPlayerGoal, MoveTowardsRestrictionGoal,
    PanicGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::callback::RemovalReason;
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::npc::merchant_state::MerchantState;
use crate::entity::{
    AgeableMob, AgeableMobBase, Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::trading::{Merchant, open_trading_screen};
use crate::world::World;

/// The three pools a wandering trader's stock comes from, in the order vanilla
/// draws them.
///
/// Vanilla parity: `WanderingTrader.updateTrades`, which draws buying first,
/// then uncommon, then common -- and that order is the order the trades appear
/// on the screen.
const TRADE_SETS: [&str; 3] = [
    "wandering_trader/buying",
    "wandering_trader/uncommon",
    "wandering_trader/common",
];

/// A trader passing through.
#[entity_behavior(class = "WanderingTrader")]
pub struct WanderingTraderEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    entity_data: SyncMutex<WanderingTraderEntityData>,
    merchant: Arc<MerchantState>,
    /// Ticks until it packs up and goes, or zero for "stays".
    ///
    /// Vanilla parity: `WanderingTrader.despawnDelay`.
    despawn_delay: SyncMutex<i32>,
    /// Where the spawner told it to head.
    ///
    /// Vanilla parity: `WanderingTrader.wanderTarget`. Foton has nothing that
    /// sets it yet -- see the note on [`Self::set_wander_target`].
    wander_target: SyncMutex<Option<BlockPos>>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `WanderingTraderEntity`.
unsafe impl DowncastType for WanderingTraderEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/wandering_trader");
}

/// The seven things a wandering trader runs from, with the distance each is
/// feared at.
///
/// Vanilla parity: the seven `AvoidEntityGoal`s of
/// `WanderingTrader.registerGoals`, in their registration order.
const AVOIDED_THREATS: &[(EntityTypeRef, f32)] = &[
    (&vanilla_entities::ZOMBIE, 8.0),
    (&vanilla_entities::EVOKER, 12.0),
    (&vanilla_entities::VINDICATOR, 8.0),
    (&vanilla_entities::VEX, 8.0),
    (&vanilla_entities::PILLAGER, 15.0),
    (&vanilla_entities::ILLUSIONER, 12.0),
    (&vanilla_entities::ZOGLIN, 10.0),
];

/// Vanilla parity: the walk speed of every `AvoidEntityGoal` above.
const FLEE_WALK_SPEED: f64 = 0.5;

/// Vanilla parity: the sprint speed of every `AvoidEntityGoal` above. Vanilla
/// passes the same number twice, so a fleeing trader never speeds up.
const FLEE_SPRINT_SPEED: f64 = 0.5;

/// Vanilla parity: the `0.5` of `new PanicGoal(this, 0.5)`.
const PANIC_SPEED: f64 = 0.5;

/// Vanilla parity: the `0.35` shared by `MoveTowardsRestrictionGoal` and
/// `WaterAvoidingRandomStrollGoal`.
const WANDER_SPEED: f64 = 0.35;

/// Vanilla parity: the `3.0F` of `new InteractGoal(this, Player.class, 3.0F, 1.0F)`.
const INTERACT_LOOK_DISTANCE: f64 = 3.0;

/// Vanilla parity: the `1.0F` of the same goal -- it always looks.
const INTERACT_PROBABILITY: f32 = 1.0;

/// Vanilla parity: the `8.0F` of `new LookAtPlayerGoal(this, Mob.class, 8.0F)`.
const LOOK_AT_MOB_DISTANCE: f64 = 8.0;

impl WanderingTraderEntity {
    /// Creates a wandering trader at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world.clone()),
            entity_type,
            id,
            world,
        )
    }

    /// Reconstructs a wandering trader from persisted base entity state.
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
        let mut entity_data = WanderingTraderEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `WanderingTrader.registerGoals`.
            //
            // MISSING FOUNDATION: vanilla also registers, at priority 0, two
            // `UseItemGoal`s -- the invisibility potion it drinks once the sky
            // is dark and the milk it drinks once the sky is bright again --
            // and at 1 a `TradeWithPlayerGoal` and a `LookAtTradingPlayerGoal`,
            // and at 2 its own `WanderToPositionGoal`. Foton has none of those
            // five goal types. The thirteen registered below are the rest.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            for (entity_type, distance) in AVOIDED_THREATS {
                goals.add_goal(
                    1,
                    AvoidEntityGoal::with_selector(
                        *distance,
                        FLEE_WALK_SPEED,
                        FLEE_SPRINT_SPEED,
                        move |_, target, _| target.entity_type() == *entity_type,
                    ),
                );
            }
            goals.add_goal(1, PanicGoal::new(PANIC_SPEED));
            goals.add_goal(4, MoveTowardsRestrictionGoal::new(WANDER_SPEED));
            goals.add_goal(8, WaterAvoidingRandomStrollGoal::new(WANDER_SPEED));
            goals.add_goal(
                9,
                InteractGoal::new_player(INTERACT_LOOK_DISTANCE, INTERACT_PROBABILITY),
            );
            goals.add_goal(10, LookAtPlayerGoal::new(LOOK_AT_MOB_DISTANCE));
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            entity_data: SyncMutex::new(entity_data),
            merchant: Arc::new(MerchantState::wandering_trader(id, world)),
            despawn_delay: SyncMutex::new(0),
            wander_target: SyncMutex::new(None),
        }
    }

    /// The trading state, for a caller that needs the `Merchant` seam.
    #[must_use]
    pub const fn merchant(&self) -> &Arc<MerchantState> {
        &self.merchant
    }

    /// Vanilla parity: `WanderingTrader.getDespawnDelay`.
    #[must_use]
    pub fn despawn_delay(&self) -> i32 {
        *self.despawn_delay.lock()
    }

    /// Vanilla parity: `WanderingTrader.setDespawnDelay`.
    pub fn set_despawn_delay(&self, delay: i32) {
        *self.despawn_delay.lock() = delay;
    }

    /// Vanilla parity: `WanderingTrader.setWanderTarget`.
    ///
    /// MISSING FOUNDATION: only `WanderingTraderSpawner` ever sets this, and
    /// Foton has no wandering-trader spawner, so nothing calls it yet. The
    /// field is here because the save format carries it and a trader loaded
    /// from a vanilla world would otherwise lose where it was heading.
    pub fn set_wander_target(&self, pos: Option<BlockPos>) {
        *self.wander_target.lock() = pos;
    }

    /// Vanilla parity: the private `WanderingTrader.getWanderTarget`.
    #[must_use]
    pub fn wander_target(&self) -> Option<BlockPos> {
        *self.wander_target.lock()
    }

    /// The offers this trader has, drawing its stock the first time it is asked.
    ///
    /// Vanilla parity: `AbstractVillager.getOffers`.
    pub fn offers(&self) -> MerchantOffers {
        if !self.merchant.offers_built() {
            self.update_trades();
            self.merchant.mark_offers_built();
        }
        self.merchant.offers().lock().clone()
    }

    /// Draws this trader's whole stock, once.
    ///
    /// Vanilla parity: `WanderingTrader.updateTrades`.
    fn update_trades(&self) {
        let mut rng = rand::rng();
        let position = self.position();
        let mut context = LootContext::new(&mut rng)
            .with_origin(position.x, position.y, position.z)
            .allowing_additional_cost_component()
            .with_this_entity(EntityRef {
                entity_type: Some(&self.entity_type.key),
                ..EntityRef::default()
            });

        let mut offers = self.merchant.offers().lock();
        for key in TRADE_SETS {
            let Some(trade_set) = REGISTRY
                .trade_sets
                .by_key(&Identifier::vanilla(key.to_string()))
            else {
                continue;
            };
            trade_set.add_offers(&mut context, &mut offers);
        }
    }

    /// Whether a player has this trader's screen open.
    #[must_use]
    pub fn is_trading(&self) -> bool {
        self.merchant.trading_player().is_some()
    }

    /// Counts the despawn delay down and leaves when it runs out.
    ///
    /// Vanilla parity: `WanderingTrader.maybeDespawn`. A trader mid-trade is
    /// never taken away from the player it is talking to.
    fn maybe_despawn(&self) {
        let expired = {
            let mut delay = self.despawn_delay.lock();
            if *delay <= 0 || self.is_trading() {
                return;
            }
            *delay -= 1;
            *delay == 0
        };
        if expired {
            self.set_removed(RemovalReason::Discarded);
        }
    }
}

impl Entity for WanderingTraderEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if self.entity_type.fixed {
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
        nbt.insert("DespawnDelay", self.despawn_delay());
        if let Some(target) = self.wander_target() {
            nbt.insert(
                "wander_target",
                NbtTag::IntArray(vec![target.x(), target.y(), target.z()]),
            );
        }
        if self.merchant.offers_built() {
            nbt.insert("Offers", offer_nbt::save(&self.merchant.offers().lock()));
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);

        if let Some(delay) = nbt.int("DespawnDelay") {
            self.set_despawn_delay(delay);
        }
        let target = nbt.int_array("wander_target").and_then(|array| {
            let [x, y, z] = array[..] else { return None };
            Some(BlockPos::new(x, y, z))
        });
        self.set_wander_target(target);
        if let Some(list) = nbt.list("Offers") {
            self.merchant.set_offers(offer_nbt::load(&list));
        }
        // Vanilla parity: `setAge(Math.max(0, getAge()))` -- a wandering trader
        // is never a baby, whatever the save said.
        AgeableMob::set_age(self, AgeableMob::get_age(self).max(0));
    }
}

impl LivingEntity for WanderingTraderEntity {
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
        Some(&sound_events::ENTITY_WANDERING_TRADER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_WANDERING_TRADER_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        self.maybe_despawn();
        result
    }
}

impl AgeableMob for WanderingTraderEntity {
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
}

impl Mob for WanderingTraderEntity {
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

    /// Vanilla parity: `WanderingTrader.removeWhenFarAway`, which is false --
    /// the despawn delay is the only thing that takes it away.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_trading() {
            &sound_events::ENTITY_WANDERING_TRADER_TRADE
        } else {
            &sound_events::ENTITY_WANDERING_TRADER_AMBIENT
        })
    }

    /// Vanilla parity: `WanderingTrader.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let stack = player.inventory.lock().get_item_in_hand(hand).clone();
        if stack.item().key.path == "villager_spawn_egg"
            || !LivingEntity::is_alive(self)
            || self.is_trading()
            || AgeableMob::is_baby(self)
        {
            return InteractionResult::Pass;
        }

        if self.offers().is_empty() {
            return InteractionResult::Consume;
        }

        self.merchant.set_trading_player(Some(player.uuid()));
        let merchant: Arc<dyn Merchant> = Arc::clone(&self.merchant) as _;
        open_trading_screen(
            &merchant,
            player,
            TextComponent::translated(TranslatedMessage {
                key: Cow::Borrowed("entity.minecraft.wandering_trader"),
                fallback: None,
                args: None,
            }),
        );
        InteractionResult::Success
    }
}

impl PathfinderMob for WanderingTraderEntity {}

#[cfg(test)]
mod goal_tests {
    use std::sync::Weak;

    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use glam::DVec3;

    use super::*;
    use crate::entity::next_entity_id;

    /// Vanilla parity: the priorities of `WanderingTrader.registerGoals`.
    ///
    /// Counted rather than merely non-empty: the shared
    /// `assert_it_has_something_to_run!` layer stays green if six of the seven
    /// flee goals disappear, because one goal is still one goal. Dropping the
    /// avoid list is exactly the regression worth catching -- a trader that
    /// lets a pillager walk up to it looks fine until you watch it.
    ///
    /// Thirteen of vanilla's eighteen. The five absent ones need goal types
    /// Foton does not have, and are named where they would be registered.
    #[test]
    fn a_wandering_trader_registers_vanillas_goal_priorities() {
        init_vanilla_registry();
        let trader = WanderingTraderEntity::new(
            &vanilla_entities::WANDERING_TRADER,
            next_entity_id(),
            DVec3::ZERO,
            Weak::new(),
        );
        let selector = trader.mob_base().goal_selector().lock();
        assert_eq!(selector.available_goal_count(), 13);
        assert_eq!(
            selector.available_goal_priorities(),
            vec![0, 1, 1, 1, 1, 1, 1, 1, 1, 4, 8, 9, 10],
            "the trader's goal list drifted from `WanderingTrader.registerGoals`"
        );
    }
}
