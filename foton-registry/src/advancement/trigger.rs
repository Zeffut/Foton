//! The criterion triggers vanilla advancement data uses.
//!
//! Vanilla parity: `CriteriaTriggers` and the `CriterionTriggerInstance` of
//! each entry. One variant per trigger id that appears in vanilla's own
//! advancement data, carrying exactly the conditions that data fills in. The
//! build script fails on a trigger id or a condition key that is not here, so
//! a criterion never silently degrades into one that asks nothing.
//!
//! Firing lives in `foton-core`. A trigger Foton does not fire yet simply
//! never awards its criterion, which is what vanilla does before the trigger
//! is invoked -- it is not the same thing as a criterion that always passes.

use foton_utils::Identifier;

use super::predicate::{
    ContextAwarePredicate, DamagePredicate, DamageSourcePredicate, DistancePredicate, IntBounds,
    ItemPredicate, LocationPredicate, StatePropertyMatch,
};

/// A check on how full an inventory is.
///
/// Vanilla parity: `InventoryChangeTrigger.TriggerInstance.Slots`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SlotsPredicate {
    /// Slots holding anything at all.
    pub occupied: IntBounds,
    /// Slots holding a stack at its maximum size.
    pub full: IntBounds,
    /// Slots holding nothing.
    pub empty: IntBounds,
}

impl SlotsPredicate {
    /// Bounds that accept every inventory.
    pub const ANY: Self = Self {
        occupied: IntBounds::ANY,
        full: IntBounds::ANY,
        empty: IntBounds::ANY,
    };
}

/// One entry of an `effects_changed` effect map.
///
/// Vanilla parity: `MobEffectsPredicate.MobEffectInstancePredicate`. Vanilla
/// advancement data leaves every entry's body empty, so presence of the effect
/// is the whole check; the build script rejects a filled-in one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MobEffectMatch {
    /// The effect that must be active.
    pub effect: Identifier,
}

/// The conditions of one criterion.
///
/// Every variant carries the `player` predicate vanilla's `SimpleCriterionTrigger`
/// puts on all of them; an empty slice is the absent, accept-anyone case.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerInstance {
    /// `minecraft:impossible`. Never fires; only a command can grant it.
    Impossible,

    /// `minecraft:tick`, fired once per player tick.
    Tick { player: ContextAwarePredicate },
    /// `minecraft:location`, fired every twenty player ticks.
    Location { player: ContextAwarePredicate },
    /// `minecraft:slept_in_bed`.
    SleptInBed { player: ContextAwarePredicate },
    /// `minecraft:hero_of_the_village`.
    HeroOfTheVillage { player: ContextAwarePredicate },
    /// `minecraft:avoid_vibration`, when a sneaking player damps a vibration.
    AvoidVibration { player: ContextAwarePredicate },
    /// `minecraft:started_riding`.
    StartedRiding { player: ContextAwarePredicate },
    /// `minecraft:enchanted_item`.
    EnchantedItem {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
        levels: IntBounds,
    },
    /// `minecraft:cured_zombie_villager`.
    CuredZombieVillager {
        player: ContextAwarePredicate,
        villager: ContextAwarePredicate,
        zombie: ContextAwarePredicate,
    },
    /// `minecraft:brewed_potion`.
    BrewedPotion {
        player: ContextAwarePredicate,
        potion: Option<Identifier>,
    },

    /// `minecraft:player_killed_entity`.
    PlayerKilledEntity {
        player: ContextAwarePredicate,
        entity: ContextAwarePredicate,
        killing_blow: Option<DamageSourcePredicate>,
    },
    /// `minecraft:entity_killed_player`.
    EntityKilledPlayer {
        player: ContextAwarePredicate,
        entity: ContextAwarePredicate,
        killing_blow: Option<DamageSourcePredicate>,
    },
    /// `minecraft:kill_mob_near_sculk_catalyst`.
    KillMobNearSculkCatalyst {
        player: ContextAwarePredicate,
        entity: ContextAwarePredicate,
        killing_blow: Option<DamageSourcePredicate>,
    },
    /// `minecraft:killed_by_arrow`.
    KilledByArrow {
        player: ContextAwarePredicate,
        /// One predicate per victim; each must be matched by a distinct victim.
        victims: &'static [ContextAwarePredicate],
        /// How many different entity types the volley had to hit.
        unique_entity_types: IntBounds,
        /// The weapon the arrow was fired from.
        fired_from_weapon: Option<ItemPredicate>,
    },
    /// `minecraft:channeled_lightning`.
    ChanneledLightning {
        player: ContextAwarePredicate,
        victims: &'static [ContextAwarePredicate],
    },
    /// `minecraft:spear_mobs`.
    SpearMobs {
        player: ContextAwarePredicate,
        /// The fewest mobs one throw had to skewer.
        ///
        /// Vanilla parity: `SpearMobsTrigger.TriggerInstance.matches` is
        /// `count.isEmpty() || speared >= count.get()`, so this is a floor,
        /// not the exact-match bounds every neighboring trigger uses.
        count: Option<i32>,
    },

    /// `minecraft:changed_dimension`.
    ChangedDimension {
        player: ContextAwarePredicate,
        from: Option<Identifier>,
        to: Option<Identifier>,
    },
    /// `minecraft:nether_travel`.
    NetherTravel {
        player: ContextAwarePredicate,
        start_position: Option<&'static LocationPredicate>,
        distance: DistancePredicate,
    },
    /// `minecraft:fall_from_height`.
    FallFromHeight {
        player: ContextAwarePredicate,
        start_position: Option<&'static LocationPredicate>,
        distance: DistancePredicate,
    },
    /// `minecraft:fall_after_explosion`.
    FallAfterExplosion {
        player: ContextAwarePredicate,
        start_position: Option<&'static LocationPredicate>,
        distance: DistancePredicate,
        /// The entity whose explosion launched the player.
        cause: ContextAwarePredicate,
    },
    /// `minecraft:ride_entity_in_lava`.
    RideEntityInLava {
        player: ContextAwarePredicate,
        start_position: Option<&'static LocationPredicate>,
        distance: DistancePredicate,
    },
    /// `minecraft:levitation`.
    Levitation {
        player: ContextAwarePredicate,
        distance: DistancePredicate,
        duration: IntBounds,
    },

    /// `minecraft:construct_beacon`.
    ConstructBeacon {
        player: ContextAwarePredicate,
        level: IntBounds,
    },
    /// `minecraft:consume_item`.
    ConsumeItem {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
    },
    /// `minecraft:effects_changed`.
    EffectsChanged {
        player: ContextAwarePredicate,
        /// Every effect that must be active at once.
        effects: &'static [MobEffectMatch],
        /// The entity that applied the effect.
        source: ContextAwarePredicate,
    },
    /// `minecraft:enter_block`.
    EnterBlock {
        player: ContextAwarePredicate,
        block: Option<Identifier>,
        state: &'static [StatePropertyMatch],
    },
    /// `minecraft:slide_down_block`.
    SlideDownBlock {
        player: ContextAwarePredicate,
        block: Option<Identifier>,
        state: &'static [StatePropertyMatch],
    },
    /// `minecraft:filled_bucket`.
    FilledBucket {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
    },
    /// `minecraft:fishing_rod_hooked`.
    FishingRodHooked {
        player: ContextAwarePredicate,
        rod: Option<ItemPredicate>,
        entity: ContextAwarePredicate,
        item: Option<ItemPredicate>,
    },
    /// `minecraft:inventory_changed`.
    InventoryChanged {
        player: ContextAwarePredicate,
        slots: SlotsPredicate,
        /// Every predicate must be satisfied by some stack in the inventory.
        items: &'static [ItemPredicate],
    },
    /// `minecraft:item_durability_changed`.
    ItemDurabilityChanged {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
        durability: IntBounds,
        delta: IntBounds,
    },
    /// `minecraft:item_used_on_block`.
    ItemUsedOnBlock {
        player: ContextAwarePredicate,
        location: ContextAwarePredicate,
    },
    /// `minecraft:allay_drop_item_on_block`.
    AllayDropItemOnBlock {
        player: ContextAwarePredicate,
        location: ContextAwarePredicate,
    },
    /// `minecraft:placed_block`.
    PlacedBlock {
        player: ContextAwarePredicate,
        location: ContextAwarePredicate,
    },
    /// `minecraft:player_generates_container_loot`.
    PlayerGeneratesContainerLoot {
        player: ContextAwarePredicate,
        loot_table: Identifier,
    },
    /// `minecraft:player_hurt_entity`.
    PlayerHurtEntity {
        player: ContextAwarePredicate,
        damage: Option<DamagePredicate>,
        entity: ContextAwarePredicate,
    },
    /// `minecraft:entity_hurt_player`.
    EntityHurtPlayer {
        player: ContextAwarePredicate,
        damage: Option<DamagePredicate>,
    },
    /// `minecraft:player_interacted_with_entity`.
    PlayerInteractedWithEntity {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
        entity: ContextAwarePredicate,
    },
    /// `minecraft:player_sheared_equipment`.
    PlayerShearedEquipment {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
        entity: ContextAwarePredicate,
    },
    /// `minecraft:recipe_crafted`.
    RecipeCrafted {
        player: ContextAwarePredicate,
        recipe_id: Identifier,
        ingredients: &'static [ItemPredicate],
    },
    /// `minecraft:crafter_recipe_crafted`.
    CrafterRecipeCrafted {
        player: ContextAwarePredicate,
        recipe_id: Identifier,
        ingredients: &'static [ItemPredicate],
    },
    /// `minecraft:recipe_unlocked`.
    RecipeUnlocked {
        player: ContextAwarePredicate,
        recipe: Identifier,
    },
    /// `minecraft:shot_crossbow`.
    ShotCrossbow {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
    },
    /// `minecraft:summoned_entity`.
    SummonedEntity {
        player: ContextAwarePredicate,
        entity: ContextAwarePredicate,
    },
    /// `minecraft:tame_animal`.
    TameAnimal {
        player: ContextAwarePredicate,
        entity: ContextAwarePredicate,
    },
    /// `minecraft:target_hit`.
    TargetHit {
        player: ContextAwarePredicate,
        signal_strength: IntBounds,
        projectile: ContextAwarePredicate,
    },
    /// `minecraft:thrown_item_picked_up_by_entity`.
    ThrownItemPickedUpByEntity {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
        entity: ContextAwarePredicate,
    },
    /// `minecraft:thrown_item_picked_up_by_player`.
    ThrownItemPickedUpByPlayer {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
        entity: ContextAwarePredicate,
    },
    /// `minecraft:used_totem`.
    UsedTotem {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
    },
    /// `minecraft:using_item`.
    UsingItem {
        player: ContextAwarePredicate,
        item: Option<ItemPredicate>,
    },
    /// `minecraft:villager_trade`.
    VillagerTrade {
        player: ContextAwarePredicate,
        villager: ContextAwarePredicate,
        item: Option<ItemPredicate>,
    },
    /// `minecraft:bee_nest_destroyed`.
    BeeNestDestroyed {
        player: ContextAwarePredicate,
        block: Option<Identifier>,
        item: Option<ItemPredicate>,
        num_bees_inside: IntBounds,
    },
    /// `minecraft:bred_animals`.
    BredAnimals {
        player: ContextAwarePredicate,
        parent: ContextAwarePredicate,
        partner: ContextAwarePredicate,
        child: ContextAwarePredicate,
    },
    /// `minecraft:lightning_strike`.
    LightningStrike {
        player: ContextAwarePredicate,
        lightning: ContextAwarePredicate,
        bystander: ContextAwarePredicate,
    },
}

impl TriggerInstance {
    /// The registry key of the trigger this instance belongs to.
    ///
    /// Vanilla parity: the `CriteriaTriggers` registration name, which is what
    /// the criterion's `trigger` field holds.
    #[must_use]
    pub const fn trigger_id(&self) -> &'static str {
        match self {
            Self::Impossible => "minecraft:impossible",
            Self::Tick { .. } => "minecraft:tick",
            Self::Location { .. } => "minecraft:location",
            Self::SleptInBed { .. } => "minecraft:slept_in_bed",
            Self::HeroOfTheVillage { .. } => "minecraft:hero_of_the_village",
            Self::AvoidVibration { .. } => "minecraft:avoid_vibration",
            Self::StartedRiding { .. } => "minecraft:started_riding",
            Self::EnchantedItem { .. } => "minecraft:enchanted_item",
            Self::CuredZombieVillager { .. } => "minecraft:cured_zombie_villager",
            Self::BrewedPotion { .. } => "minecraft:brewed_potion",
            Self::PlayerKilledEntity { .. } => "minecraft:player_killed_entity",
            Self::EntityKilledPlayer { .. } => "minecraft:entity_killed_player",
            Self::KillMobNearSculkCatalyst { .. } => "minecraft:kill_mob_near_sculk_catalyst",
            Self::KilledByArrow { .. } => "minecraft:killed_by_arrow",
            Self::ChanneledLightning { .. } => "minecraft:channeled_lightning",
            Self::SpearMobs { .. } => "minecraft:spear_mobs",
            Self::ChangedDimension { .. } => "minecraft:changed_dimension",
            Self::NetherTravel { .. } => "minecraft:nether_travel",
            Self::FallFromHeight { .. } => "minecraft:fall_from_height",
            Self::FallAfterExplosion { .. } => "minecraft:fall_after_explosion",
            Self::RideEntityInLava { .. } => "minecraft:ride_entity_in_lava",
            Self::Levitation { .. } => "minecraft:levitation",
            Self::ConstructBeacon { .. } => "minecraft:construct_beacon",
            Self::ConsumeItem { .. } => "minecraft:consume_item",
            Self::EffectsChanged { .. } => "minecraft:effects_changed",
            Self::EnterBlock { .. } => "minecraft:enter_block",
            Self::SlideDownBlock { .. } => "minecraft:slide_down_block",
            Self::FilledBucket { .. } => "minecraft:filled_bucket",
            Self::FishingRodHooked { .. } => "minecraft:fishing_rod_hooked",
            Self::InventoryChanged { .. } => "minecraft:inventory_changed",
            Self::ItemDurabilityChanged { .. } => "minecraft:item_durability_changed",
            Self::ItemUsedOnBlock { .. } => "minecraft:item_used_on_block",
            Self::AllayDropItemOnBlock { .. } => "minecraft:allay_drop_item_on_block",
            Self::PlacedBlock { .. } => "minecraft:placed_block",
            Self::PlayerGeneratesContainerLoot { .. } => {
                "minecraft:player_generates_container_loot"
            }
            Self::PlayerHurtEntity { .. } => "minecraft:player_hurt_entity",
            Self::EntityHurtPlayer { .. } => "minecraft:entity_hurt_player",
            Self::PlayerInteractedWithEntity { .. } => "minecraft:player_interacted_with_entity",
            Self::PlayerShearedEquipment { .. } => "minecraft:player_sheared_equipment",
            Self::RecipeCrafted { .. } => "minecraft:recipe_crafted",
            Self::CrafterRecipeCrafted { .. } => "minecraft:crafter_recipe_crafted",
            Self::RecipeUnlocked { .. } => "minecraft:recipe_unlocked",
            Self::ShotCrossbow { .. } => "minecraft:shot_crossbow",
            Self::SummonedEntity { .. } => "minecraft:summoned_entity",
            Self::TameAnimal { .. } => "minecraft:tame_animal",
            Self::TargetHit { .. } => "minecraft:target_hit",
            Self::ThrownItemPickedUpByEntity { .. } => "minecraft:thrown_item_picked_up_by_entity",
            Self::ThrownItemPickedUpByPlayer { .. } => "minecraft:thrown_item_picked_up_by_player",
            Self::UsedTotem { .. } => "minecraft:used_totem",
            Self::UsingItem { .. } => "minecraft:using_item",
            Self::VillagerTrade { .. } => "minecraft:villager_trade",
            Self::BeeNestDestroyed { .. } => "minecraft:bee_nest_destroyed",
            Self::BredAnimals { .. } => "minecraft:bred_animals",
            Self::LightningStrike { .. } => "minecraft:lightning_strike",
        }
    }

    /// The `player` predicate every trigger instance carries.
    #[must_use]
    pub const fn player(&self) -> ContextAwarePredicate {
        match self {
            Self::Impossible => &[],
            Self::Tick { player }
            | Self::Location { player }
            | Self::SleptInBed { player }
            | Self::HeroOfTheVillage { player }
            | Self::AvoidVibration { player }
            | Self::StartedRiding { player }
            | Self::EnchantedItem { player, .. }
            | Self::CuredZombieVillager { player, .. }
            | Self::BrewedPotion { player, .. }
            | Self::PlayerKilledEntity { player, .. }
            | Self::EntityKilledPlayer { player, .. }
            | Self::KillMobNearSculkCatalyst { player, .. }
            | Self::KilledByArrow { player, .. }
            | Self::ChanneledLightning { player, .. }
            | Self::SpearMobs { player, .. }
            | Self::ChangedDimension { player, .. }
            | Self::NetherTravel { player, .. }
            | Self::FallFromHeight { player, .. }
            | Self::FallAfterExplosion { player, .. }
            | Self::RideEntityInLava { player, .. }
            | Self::Levitation { player, .. }
            | Self::ConstructBeacon { player, .. }
            | Self::ConsumeItem { player, .. }
            | Self::EffectsChanged { player, .. }
            | Self::EnterBlock { player, .. }
            | Self::SlideDownBlock { player, .. }
            | Self::FilledBucket { player, .. }
            | Self::FishingRodHooked { player, .. }
            | Self::InventoryChanged { player, .. }
            | Self::ItemDurabilityChanged { player, .. }
            | Self::ItemUsedOnBlock { player, .. }
            | Self::AllayDropItemOnBlock { player, .. }
            | Self::PlacedBlock { player, .. }
            | Self::PlayerGeneratesContainerLoot { player, .. }
            | Self::PlayerHurtEntity { player, .. }
            | Self::EntityHurtPlayer { player, .. }
            | Self::PlayerInteractedWithEntity { player, .. }
            | Self::PlayerShearedEquipment { player, .. }
            | Self::RecipeCrafted { player, .. }
            | Self::CrafterRecipeCrafted { player, .. }
            | Self::RecipeUnlocked { player, .. }
            | Self::ShotCrossbow { player, .. }
            | Self::SummonedEntity { player, .. }
            | Self::TameAnimal { player, .. }
            | Self::TargetHit { player, .. }
            | Self::ThrownItemPickedUpByEntity { player, .. }
            | Self::ThrownItemPickedUpByPlayer { player, .. }
            | Self::UsedTotem { player, .. }
            | Self::UsingItem { player, .. }
            | Self::VillagerTrade { player, .. }
            | Self::BeeNestDestroyed { player, .. }
            | Self::BredAnimals { player, .. }
            | Self::LightningStrike { player, .. } => player,
        }
    }
}
