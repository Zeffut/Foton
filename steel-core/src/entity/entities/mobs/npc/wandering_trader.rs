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

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::loot_table::{EntityRef, LootContext};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::trading::{MerchantOffers, offer_nbt};
use steel_registry::vanilla_entity_data::WanderingTraderEntityData;
use steel_registry::{REGISTRY, RegistryExt as _, sound_events};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey, Identifier};
use text_components::TextComponent;
use text_components::translation::TranslatedMessage;

use crate::behavior::InteractionResult;
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
    /// Vanilla parity: `WanderingTrader.wanderTarget`. Steel has nothing that
    /// sets it yet -- see the note on [`Self::set_wander_target`].
    wander_target: SyncMutex<Option<BlockPos>>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `WanderingTraderEntity`.
unsafe impl DowncastType for WanderingTraderEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/wandering_trader");
}

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
    /// Steel has no wandering-trader spawner, so nothing calls it yet. The
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
