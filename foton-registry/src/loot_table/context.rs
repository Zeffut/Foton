use super::{BlockStateId, DyeColor, Identifier, ItemStack, REGISTRY, RegistryExt, RngExt};
use crate::biome::BiomeRef;
use crate::data_components::DataComponentMap;
use crate::equipment::EquipmentSlot;
use crate::loot_table::functions::CopySource;

/// The live world a loot roll is allowed to ask about.
///
/// Vanilla parity: the `ServerLevel` every `LootContext` carries.
/// `foton-registry` cannot see `foton-core`'s world, so the two facts loot
/// actually reads -- the block and the biome at a position -- come in through
/// this trait instead.
///
/// Both answers are `None` for a position vanilla's `Level.isLoaded` would
/// reject. `LocationPredicate.matches` fails there rather than guessing, so
/// the distinction has to survive the trait boundary.
pub trait LootWorldView {
    /// The block state at a block position, `None` when it is not loaded.
    fn loaded_block_state(&self, x: i32, y: i32, z: i32) -> Option<BlockStateId>;

    /// The biome at a block position, `None` when it is not loaded.
    fn loaded_biome(&self, x: i32, y: i32, z: i32) -> Option<BiomeRef>;
}

/// Entity target for loot context lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootContextEntity {
    /// The entity being looted (killed mob, block entity owner).
    This,
    /// The entity that killed the target.
    Killer,
    /// The direct attacker (e.g., arrow, not the player who shot it).
    DirectKiller,
    /// The player who dealt the final damage.
    KillerPlayer,
    /// The entity interacting with a block/entity.
    Interacting,
}

/// The type of loot table, determining when/how it's used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootType {
    Block,
    Entity,
    Chest,
    Fishing,
    Gift,
    Archaeology,
    Vault,
    Shearing,
    Equipment,
    Selector,
    EntityInteract,
    BlockInteract,
    Barter,
}

/// A number provider that can be constant or random.
#[derive(Debug, Clone)]
pub enum NumberProvider {
    Constant(f32),
    Uniform {
        min: f32,
        max: f32,
    },
    Binomial {
        n: i32,
        p: f32,
    },
    /// Get value from entity scoreboard score.
    Score {
        target: ScoreboardTarget,
        score: &'static str,
        scale: f32,
    },
    /// Get value from command storage.
    Storage {
        storage: Identifier,
        path: &'static str,
    },
    /// Get enchantment level from context tool.
    EnchantmentLevel {
        enchantment: Identifier,
    },
    /// The sum of several providers.
    ///
    /// Vanilla parity: `net.minecraft.world.level.storage.loot.providers.number.Sum`.
    /// Note that it adds its summands as *floats* and only then converts, so
    /// `sum(0.6, 0.6)` is one, not two; and that its `getInt` floors where every
    /// other provider rounds.
    Sum(&'static [NumberProvider]),
}

/// Target for scoreboard number provider.
#[derive(Debug, Clone, Copy)]
pub enum ScoreboardTarget {
    /// The entity being looted.
    This,
    /// The entity that killed the target.
    Killer,
    /// The direct killer (e.g., arrow vs player).
    DirectKiller,
    /// The player who dealt the last damage.
    KillerPlayer,
    /// A fixed entity name.
    Fixed(&'static str),
}

impl NumberProvider {
    /// Get a value from this provider using the given RNG.
    pub fn get<R: rand::Rng>(&self, rng: &mut R, ctx: Option<&LootContextRef<'_>>) -> f32 {
        match self {
            Self::Constant(v) => *v,
            Self::Uniform { min, max } => rng.random_range(*min..=*max),
            Self::Binomial { n, p } => {
                let mut count = 0;
                for _ in 0..*n {
                    if rng.random::<f32>() < *p {
                        count += 1;
                    }
                }
                count as f32
            }
            Self::Score { .. } => {
                // TODO: Implement when scoreboard system is available
                let _ = ctx;
                0.0
            }
            Self::Storage { .. } => {
                // TODO: Implement when command storage system is available
                let _ = ctx;
                0.0
            }
            Self::EnchantmentLevel { enchantment } => ctx
                .and_then(|c| c.tool)
                .map_or(0.0, |t| t.get_enchantment_level(enchantment) as f32),
            Self::Sum(summands) => summands
                .iter()
                .map(|summand| summand.get(rng, ctx))
                .sum::<f32>(),
        }
    }

    /// Get a value without context (for backwards compatibility).
    pub fn get_simple(&self, rng: &mut impl rand::Rng) -> f32 {
        match self {
            Self::Constant(v) => *v,
            Self::Uniform { min, max } => rng.random_range(*min..=*max),
            Self::Binomial { n, p } => {
                let mut count = 0;
                for _ in 0..*n {
                    if rng.random::<f32>() < *p {
                        count += 1;
                    }
                }
                count as f32
            }
            Self::Sum(summands) => summands
                .iter()
                .map(|summand| summand.get_simple(rng))
                .sum::<f32>(),
            // Context-dependent providers return 0 when no context available
            Self::Score { .. } | Self::Storage { .. } | Self::EnchantmentLevel { .. } => 0.0,
        }
    }

    /// Get the value as an integer.
    pub fn get_int(&self, rng: &mut impl rand::Rng) -> i32 {
        match self {
            Self::Uniform { min, max } => uniform_int(rng, math_round(*min), math_round(*max)),
            // `Sum.getInt` is the one provider that floors rather than rounds.
            Self::Sum(_) => math_floor(self.get_simple(rng)),
            other => math_round(other.get_simple(rng)),
        }
    }

    /// Get the value as an integer with context.
    pub fn get_int_with_ctx<R: rand::Rng>(
        &self,
        rng: &mut R,
        ctx: Option<&LootContextRef<'_>>,
    ) -> i32 {
        match self {
            Self::Uniform { min, max } => uniform_int(rng, math_round(*min), math_round(*max)),
            Self::Sum(_) => math_floor(self.get(rng, ctx)),
            other => math_round(other.get(rng, ctx)),
        }
    }
}

/// Vanilla parity: `Mth.floor(float)`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "vanilla's Mth.floor(float) truncates to int the same way"
)]
#[must_use]
pub const fn math_floor(value: f32) -> i32 {
    value.floor() as i32
}

/// `java.lang.Math.round` semantics for a float.
pub(super) fn math_round(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

/// Vanilla `Mth.nextInt(random, min, max)` is inclusive and clamps to `min`
/// when `min >= max`.
pub(super) fn uniform_int(rng: &mut impl rand::Rng, min: i32, max: i32) -> i32 {
    if min >= max {
        min
    } else {
        rng.random_range(min..=max)
    }
}

/// A range for number comparisons (used in `ValueCheck`, `TimeCheck`, `EntityScores`).
#[derive(Debug, Clone)]
pub struct NumberProviderRange {
    pub min: Option<NumberProvider>,
    pub max: Option<NumberProvider>,
}

impl NumberProviderRange {
    /// Check if a value is within this range.
    pub fn test(&self, value: f32, rng: &mut impl rand::Rng) -> bool {
        if let Some(min) = &self.min
            && value < min.get_simple(rng)
        {
            return false;
        }
        if let Some(max) = &self.max
            && value > max.get_simple(rng)
        {
            return false;
        }
        true
    }

    /// Check if a value is within this range, for callers with no RNG.
    ///
    /// Constant bounds need no randomness; a bound that is not constant cannot
    /// be resolved here and rejects the value rather than guessing.
    #[must_use]
    pub fn test_without_random(&self, value: f32) -> bool {
        const fn constant(provider: &NumberProvider) -> Option<f32> {
            match provider {
                NumberProvider::Constant(value) => Some(*value),
                _ => None,
            }
        }

        if let Some(min) = &self.min {
            let Some(min) = constant(min) else {
                return false;
            };
            if value < min {
                return false;
            }
        }
        if let Some(max) = &self.max {
            let Some(max) = constant(max) else {
                return false;
            };
            if value > max {
                return false;
            }
        }
        true
    }

    /// Create an exact match range.
    #[must_use]
    pub const fn exact(value: f32) -> Self {
        Self {
            min: Some(NumberProvider::Constant(value)),
            max: Some(NumberProvider::Constant(value)),
        }
    }

    /// Create an at-least range.
    #[must_use]
    pub const fn at_least(min: f32) -> Self {
        Self {
            min: Some(NumberProvider::Constant(min)),
            max: None,
        }
    }

    /// Create an at-most range.
    #[must_use]
    pub const fn at_most(max: f32) -> Self {
        Self {
            min: None,
            max: Some(NumberProvider::Constant(max)),
        }
    }

    /// Create a between range.
    #[must_use]
    pub const fn between(min: f32, max: f32) -> Self {
        Self {
            min: Some(NumberProvider::Constant(min)),
            max: Some(NumberProvider::Constant(max)),
        }
    }
}

/// Reference to loot context for number provider evaluation.
/// This is a simplified view to avoid circular references.
pub struct LootContextRef<'a> {
    pub tool: Option<&'a ItemStack>,
    // Add more fields as needed for Score/Storage providers
}

/// Context for loot table evaluation, containing all relevant game state.
///
/// This mirrors vanilla's `LootContext` / `LootParams` system.
pub struct LootContext<'a, R: rand::Rng> {
    /// Random number generator.
    pub rng: &'a mut R,
    /// Luck value (e.g., from Luck of the Sea enchantment).
    pub luck: f32,
    /// The block state being broken (for block loot tables).
    pub block_state: Option<BlockStateId>,
    /// The tool used to break the block.
    pub tool: Option<&'a ItemStack>,
    /// Explosion radius if caused by an explosion.
    pub explosion_radius: Option<f32>,
    /// Whether the entity was killed by a player.
    pub killed_by_player: bool,

    /// World position where the loot is generated (block position or entity death location).
    pub origin: Option<(f64, f64, f64)>,
    /// Current game time in ticks (for `TimeCheck` condition).
    pub game_time: Option<i64>,
    /// Current weather state.
    pub weather: Option<WeatherState>,
    /// The entity being looted (the killed mob, block entity owner, etc.).
    /// This is a type-erased pointer; actual entity data depends on game implementation.
    pub this_entity: Option<EntityRef<'a>>,
    /// The entity that killed `this_entity` (for mob loot tables).
    pub killer_entity: Option<EntityRef<'a>>,
    /// The direct attacker entity (e.g., arrow, not the player who shot it).
    pub direct_killer_entity: Option<EntityRef<'a>>,
    /// The player who dealt the final damage (may be different from killer).
    pub last_damage_player: Option<EntityRef<'a>>,
    /// Damage source information for entity deaths.
    pub damage_source: Option<DamageSourceInfo<'a>>,
    /// Block entity reference for container/block loot.
    pub block_entity: Option<BlockEntityRef<'a>>,
    /// The entity interacting with a block/entity (e.g., player opening a chest).
    pub interacting_entity: Option<EntityRef<'a>>,
    /// The world the loot is being rolled in.
    ///
    /// Vanilla parity: `LootContext.getLevel`. Absent means no world could be
    /// reached, which fails every predicate that needs one.
    pub world: Option<&'a dyn LootWorldView>,
    /// Whether an enchanting function may bank its cost in `ADDITIONAL_TRADE_COST`.
    ///
    /// Vanilla parity: the presence of `LootContextParams.ADDITIONAL_COST_COMPONENT_ALLOWED`,
    /// which only `AbstractVillager.addOffersFromTradeSet` supplies. Every other
    /// loot roll leaves it out, so a chest's enchanted sword never carries a price.
    pub additional_cost_component_allowed: bool,
}

/// Weather state for `WeatherCheck` condition.
#[derive(Debug, Clone, Copy, Default)]
pub struct WeatherState {
    pub raining: bool,
    pub thundering: bool,
}

/// A reference to an entity for loot context.
/// This is intentionally opaque - the actual entity type depends on game implementation.
///
/// The default is an entity nothing is known about: every predicate that asks
/// for a specific fact fails against it.
#[derive(Debug, Clone, Copy, Default)]
pub struct EntityRef<'a> {
    /// Type identifier for the entity.
    pub entity_type: Option<&'a Identifier>,
    /// Entity flags for predicate checking.
    pub flags: EntityRefFlags,
    /// Equipment slots for equipment predicates.
    pub equipment: Option<&'a EntityEquipmentRef<'a>>,
    /// Entity name (for `copy_name` function).
    pub custom_name: Option<&'a str>,
    /// Vanilla `minecraft:components.sheep/color` entity data component.
    pub sheep_color: Option<DyeColor>,
    /// Vanilla `minecraft:type_specific/sheep.sheared`. `None` when the entity is
    /// not a sheep, matching vanilla `SheepPredicate.matches`' non-sheep rejection.
    pub sheep_sheared: Option<bool>,
    /// Vanilla `minecraft:components.chicken/variant`, `None` for non-chickens.
    pub chicken_variant: Option<&'a Identifier>,
    /// Vanilla `minecraft:components.frog/variant`, `None` for non-frogs. This
    /// is what decides which froglight a magma cube leaves behind.
    pub frog_variant: Option<&'a Identifier>,
    /// Vanilla `minecraft:components.mooshroom/variant` by serialized name,
    /// `None` for non-mooshrooms.
    pub mooshroom_variant: Option<&'static str>,
    /// Vanilla `minecraft:type_specific/cube_mob.size`, `None` for anything that
    /// is not a slime or magma cube.
    pub cube_size: Option<i32>,
    /// Vanilla `FishingHook.isOpenWaterFishing`, `None` for anything that is not
    /// a fishing hook.
    pub in_open_water: Option<bool>,
    /// Vanilla `minecraft:type_specific/raider`, `None` for anything that is
    /// not a raider. A patrol captain outside a raid is what drops the ominous
    /// bottle.
    pub raider: Option<RaiderStatus>,
    /// The type of whatever this entity is riding, `None` when it rides
    /// nothing.
    ///
    /// Vanilla's `EntityPredicate.vehicle` is a whole nested predicate; the
    /// loot data only ever asks the vehicle's type, and the build script
    /// refuses a vehicle predicate that asks for more.
    pub vehicle_type: Option<&'a Identifier>,
    /// Vanilla `minecraft:predicates.villager/variant`, the villager type a
    /// villager or zombie villager answers `DataComponents.VILLAGER_VARIANT`
    /// with. `None` for anything that is not one. This is what decides which
    /// boats a fisherman sells and which maps a cartographer draws.
    pub villager_variant: Option<&'a Identifier>,
}

/// What vanilla's `RaiderPredicate` reads off a `Raider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaiderStatus {
    /// Vanilla `Raider.hasRaid`.
    pub has_raid: bool,
    /// Vanilla `Raider.isCaptain`: wearing the ominous banner as a patrol
    /// leader or a raid captain.
    pub is_captain: bool,
}

/// Entity flags for predicate checking.
#[derive(Debug, Clone, Copy, Default)]
pub struct EntityRefFlags {
    pub is_on_fire: bool,
    pub is_sneaking: bool,
    pub is_sprinting: bool,
    pub is_swimming: bool,
    pub is_baby: bool,
}

/// Equipment references for an entity.
#[derive(Debug, Clone, Copy)]
pub struct EntityEquipmentRef<'a> {
    pub mainhand: Option<&'a ItemStack>,
    pub offhand: Option<&'a ItemStack>,
    pub head: Option<&'a ItemStack>,
    pub chest: Option<&'a ItemStack>,
    pub legs: Option<&'a ItemStack>,
    pub feet: Option<&'a ItemStack>,
}

impl<'a> EntityEquipmentRef<'a> {
    /// The occupied slots, paired with the stack in them.
    ///
    /// Vanilla iterates `EquipmentSlot.VALUES` and skips empty stacks, which is
    /// what `Enchantment.getSlotItems` relies on.
    pub fn occupied_slots(&self) -> impl Iterator<Item = (EquipmentSlot, &'a ItemStack)> + '_ {
        [
            (EquipmentSlot::MainHand, self.mainhand),
            (EquipmentSlot::OffHand, self.offhand),
            (EquipmentSlot::Feet, self.feet),
            (EquipmentSlot::Legs, self.legs),
            (EquipmentSlot::Chest, self.chest),
            (EquipmentSlot::Head, self.head),
        ]
        .into_iter()
        .filter_map(|(slot, stack)| stack.map(|stack| (slot, stack)))
    }
}

/// Damage source information for loot context.
#[derive(Debug, Clone, Copy)]
pub struct DamageSourceInfo<'a> {
    /// The damage type identifier.
    pub damage_type: Option<&'a Identifier>,
    /// Tags associated with this damage source.
    pub tags: &'a [Identifier],
    /// Whether this is direct damage (not from a projectile).
    pub is_direct: bool,
}

/// The block entity behind `LootContextParams.BLOCK_ENTITY`.
///
/// Vanilla hands the live `BlockEntity` to the roll, but every loot function
/// that names it only ever asks for `collectComponents()`. `foton-registry`
/// cannot see a block entity, so that map is what crosses the crate boundary.
#[derive(Debug, Clone, Copy)]
pub struct BlockEntityRef<'a> {
    /// Vanilla parity: `BlockEntity.collectComponents()`.
    pub components: &'a DataComponentMap,
}

impl<'a, R: rand::Rng> LootContext<'a, R> {
    /// Create a new loot context with just an RNG.
    pub const fn new(rng: &'a mut R) -> Self {
        Self {
            rng,
            luck: 0.0,
            block_state: None,
            tool: None,
            explosion_radius: None,
            killed_by_player: false,
            origin: None,
            game_time: None,
            weather: None,
            this_entity: None,
            killer_entity: None,
            direct_killer_entity: None,
            last_damage_player: None,
            damage_source: None,
            block_entity: None,
            interacting_entity: None,
            world: None,
            additional_cost_component_allowed: false,
        }
    }

    /// Set the world the loot is rolled in.
    #[must_use]
    pub const fn with_world(mut self, world: &'a dyn LootWorldView) -> Self {
        self.world = Some(world);
        self
    }

    /// Allow enchanting functions to bank their cost in `ADDITIONAL_TRADE_COST`.
    ///
    /// Vanilla parity: adding `LootContextParams.ADDITIONAL_COST_COMPONENT_ALLOWED`
    /// to the params, which only a merchant building its offers does.
    #[must_use]
    pub const fn allowing_additional_cost_component(mut self) -> Self {
        self.additional_cost_component_allowed = true;
        self
    }

    /// Set the luck value.
    #[must_use]
    pub const fn with_luck(mut self, luck: f32) -> Self {
        self.luck = luck;
        self
    }

    /// Set the block state.
    #[must_use]
    pub const fn with_block_state(mut self, state: BlockStateId) -> Self {
        self.block_state = Some(state);
        self
    }

    /// Set the tool used.
    #[must_use]
    pub const fn with_tool(mut self, tool: &'a ItemStack) -> Self {
        self.tool = Some(tool);
        self
    }

    /// Set the explosion radius.
    #[must_use]
    pub const fn with_explosion(mut self, radius: f32) -> Self {
        self.explosion_radius = Some(radius);
        self
    }

    /// Set whether killed by player.
    #[must_use]
    pub const fn with_killed_by_player(mut self, killed: bool) -> Self {
        self.killed_by_player = killed;
        self
    }

    /// Set the world origin position.
    #[must_use]
    pub const fn with_origin(mut self, x: f64, y: f64, z: f64) -> Self {
        self.origin = Some((x, y, z));
        self
    }

    /// Set the game time.
    #[must_use]
    pub const fn with_game_time(mut self, time: i64) -> Self {
        self.game_time = Some(time);
        self
    }

    /// Set the weather state.
    #[must_use]
    pub const fn with_weather(mut self, weather: WeatherState) -> Self {
        self.weather = Some(weather);
        self
    }

    /// Set the entity being looted.
    #[must_use]
    pub const fn with_this_entity(mut self, entity: EntityRef<'a>) -> Self {
        self.this_entity = Some(entity);
        self
    }

    /// Set the killer entity.
    #[must_use]
    pub const fn with_killer_entity(mut self, entity: EntityRef<'a>) -> Self {
        self.killer_entity = Some(entity);
        self
    }

    /// Set the direct killer entity (e.g., projectile).
    #[must_use]
    pub const fn with_direct_killer_entity(mut self, entity: EntityRef<'a>) -> Self {
        self.direct_killer_entity = Some(entity);
        self
    }

    /// Set the player who dealt the final damage.
    #[must_use]
    pub const fn with_last_damage_player(mut self, entity: EntityRef<'a>) -> Self {
        self.last_damage_player = Some(entity);
        self
    }

    /// Set the damage source information.
    #[must_use]
    pub const fn with_damage_source(mut self, damage_source: DamageSourceInfo<'a>) -> Self {
        self.damage_source = Some(damage_source);
        self
    }

    /// Set the block entity reference.
    #[must_use]
    pub const fn with_block_entity(mut self, block_entity: BlockEntityRef<'a>) -> Self {
        self.block_entity = Some(block_entity);
        self
    }

    /// Set the interacting entity (e.g., player opening a chest).
    #[must_use]
    pub const fn with_interacting_entity(mut self, entity: EntityRef<'a>) -> Self {
        self.interacting_entity = Some(entity);
        self
    }

    /// Get the level of an enchantment on the tool by identifier.
    #[must_use]
    pub fn get_enchantment_level_by_id(&self, enchantment: &Identifier) -> i32 {
        self.tool
            .map_or(0, |t| t.get_enchantment_level(enchantment))
    }

    /// Vanilla `EnchantmentHelper.getEnchantmentLevel(enchantment, livingEntity)`:
    /// the best level of `enchantment` across the equipment slots that
    /// enchantment is allowed to occupy.
    ///
    /// This is what the looting-style loot primitives read. They take the level
    /// off the *killer*, not off `TOOL` -- an entity loot roll has no `TOOL`
    /// parameter at all.
    #[must_use]
    pub fn get_entity_enchantment_level(
        &self,
        target: LootContextEntity,
        enchantment: &Identifier,
    ) -> i32 {
        let Some(entity) = self.get_entity(target) else {
            return 0;
        };
        let Some(equipment) = entity.equipment else {
            return 0;
        };
        let Some(definition) = REGISTRY.enchantments.by_key(enchantment) else {
            return 0;
        };

        equipment
            .occupied_slots()
            .filter(|(slot, _)| definition.slots.iter().any(|group| group.test(*slot)))
            .map(|(_, stack)| stack.get_enchantment_level(enchantment))
            .max()
            .unwrap_or(0)
    }

    /// The components a `copy_components`/`copy_name` source hands out.
    ///
    /// Vanilla resolves the source through `LootContextArg`: a block entity
    /// answers with `collectComponents()`, while an entity or an item stack is
    /// itself a `DataComponentGetter`.
    ///
    /// MISSING FOUNDATION: Foton's `EntityRef` carries no component map, so the
    /// three entity sources answer nothing. No vanilla loot table names one --
    /// all 71 `copy_components` uses in 26.2 read `block_entity`.
    #[must_use]
    pub const fn copy_source_components(&self, source: CopySource) -> Option<&'a DataComponentMap> {
        match source {
            CopySource::BlockEntity => match self.block_entity {
                Some(block_entity) => Some(block_entity.components),
                None => None,
            },
            CopySource::This | CopySource::Attacker | CopySource::DirectAttacker => None,
        }
    }

    /// Get an entity reference by target.
    #[must_use]
    pub const fn get_entity(&self, target: LootContextEntity) -> Option<EntityRef<'a>> {
        match target {
            LootContextEntity::This => self.this_entity,
            LootContextEntity::Killer => self.killer_entity,
            LootContextEntity::DirectKiller => self.direct_killer_entity,
            LootContextEntity::KillerPlayer => self.last_damage_player,
            LootContextEntity::Interacting => self.interacting_entity,
        }
    }
}
