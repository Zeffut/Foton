//! The predicates an advancement criterion is allowed to ask about.
//!
//! Vanilla parity: `net.minecraft.advancements.predicates` plus the
//! `LootItemCondition`s a `ContextAwarePredicate` is made of. Vanilla reuses the
//! loot condition machinery here, and so does Steel -- but the loot version in
//! [`crate::loot_table::conditions`] evaluates itself against a `LootContext`
//! that only knows what a loot roll knows. An advancement asks about vehicles,
//! passengers, what a player is looking at and how far away a victim died, none
//! of which a loot context carries, so these are a separate, plain-data model.
//! Evaluation lives in `steel-core`, where the world is reachable.
//!
//! Only the shapes vanilla's own advancement data actually uses are modeled.
//! The build script fails on anything else rather than emitting a predicate
//! that asks nothing -- an advancement handed out for the wrong kill is worse
//! than one that never fires.

use std::fmt::Debug;

use steel_utils::Identifier;

/// An inclusive numeric range, either bound optional.
///
/// Vanilla parity: `MinMaxBounds.Doubles`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DoubleBounds {
    /// The lowest accepted value, `None` for unbounded.
    pub min: Option<f64>,
    /// The highest accepted value, `None` for unbounded.
    pub max: Option<f64>,
}

impl DoubleBounds {
    /// Bounds that accept every value.
    pub const ANY: Self = Self {
        min: None,
        max: None,
    };

    /// Whether neither bound was given.
    #[must_use]
    pub const fn is_any(&self) -> bool {
        self.min.is_none() && self.max.is_none()
    }

    /// Whether `value` falls inside the range.
    #[must_use]
    pub fn matches(&self, value: f64) -> bool {
        if let Some(min) = self.min
            && value < min
        {
            return false;
        }
        if let Some(max) = self.max
            && value > max
        {
            return false;
        }
        true
    }

    /// Whether the square of `value` falls inside the squared range.
    ///
    /// Vanilla parity: `MinMaxBounds.Doubles.matchesSqr`, which is what
    /// `DistancePredicate` uses so it never takes a square root.
    #[must_use]
    pub fn matches_sqr(&self, value_sqr: f64) -> bool {
        if let Some(min) = self.min
            && value_sqr < min * min
        {
            return false;
        }
        if let Some(max) = self.max
            && value_sqr > max * max
        {
            return false;
        }
        true
    }
}

/// An inclusive integer range, either bound optional.
///
/// Vanilla parity: `MinMaxBounds.Ints`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IntBounds {
    /// The lowest accepted value, `None` for unbounded.
    pub min: Option<i32>,
    /// The highest accepted value, `None` for unbounded.
    pub max: Option<i32>,
}

impl IntBounds {
    /// Bounds that accept every value.
    pub const ANY: Self = Self {
        min: None,
        max: None,
    };

    /// Whether neither bound was given.
    #[must_use]
    pub const fn is_any(&self) -> bool {
        self.min.is_none() && self.max.is_none()
    }

    /// Whether `value` falls inside the range.
    #[must_use]
    pub const fn matches(&self, value: i32) -> bool {
        if let Some(min) = self.min
            && value < min
        {
            return false;
        }
        if let Some(max) = self.max
            && value > max
        {
            return false;
        }
        true
    }
}

/// A registry set written either as a tag or as an explicit list.
///
/// Vanilla parity: the `HolderSet` codec every `items` / `blocks` / `biomes`
/// field uses, which accepts `"#tag"`, `"single:id"` or `["a", "b"]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrySet {
    /// Every entry carrying this tag.
    Tag(Identifier),
    /// Exactly these entries.
    Entries(&'static [Identifier]),
}

impl RegistrySet {
    /// Whether `key` is one of the explicit entries, ignoring tags.
    ///
    /// Tag membership needs a registry and is resolved by the caller.
    #[must_use]
    pub fn contains_key(&self, key: &Identifier) -> bool {
        match self {
            Self::Tag(_) => false,
            Self::Entries(entries) => entries.contains(key),
        }
    }
}

/// A block state property comparison, by serialized name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatePropertyMatch {
    /// The property's name in the block state.
    pub name: &'static str,
    /// The value the property must serialize to.
    pub value: &'static str,
}

/// A check on the block at a position.
///
/// Vanilla parity: `BlockPredicate`. Only `blocks` and exact-value `state`
/// entries appear in vanilla advancement data; ranged state values and NBT
/// checks do not, and the build script rejects them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPredicate {
    /// The blocks accepted, `None` for any block.
    pub blocks: Option<RegistrySet>,
    /// Properties that must hold, all of them.
    pub state: &'static [StatePropertyMatch],
}

impl BlockPredicate {
    /// A predicate that accepts every block.
    pub const ANY: Self = Self {
        blocks: None,
        state: &[],
    };
}

/// An enchantment level check on an item.
///
/// Vanilla parity: `EnchantmentsPredicate.Enchantments`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnchantmentPredicate {
    /// The enchantments accepted, `None` for any enchantment.
    pub enchantments: Option<RegistrySet>,
    /// The level the enchantment must be at.
    pub levels: IntBounds,
}

/// A banner pattern layer, as written in a `minecraft:banner_patterns` check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BannerPatternLayer {
    /// The dye color's serialized name.
    pub color: &'static str,
    /// The banner pattern's registry key.
    pub pattern: Identifier,
}

/// The item data components a vanilla advancement checks.
///
/// Vanilla parity: the `components` map of `ItemPredicate`, whose codec is
/// open-ended. Steel models exactly the three keys vanilla data uses; the
/// build script fails on a fourth rather than ignoring it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemComponentsPredicate {
    /// `minecraft:damage` must equal this.
    pub damage: Option<i32>,
    /// `minecraft:banner_patterns` must be exactly these layers, in order.
    pub banner_patterns: Option<&'static [BannerPatternLayer]>,
    /// `minecraft:item_name` must be this translatable component's key.
    pub item_name_translate: Option<&'static str>,
}

impl ItemComponentsPredicate {
    /// A predicate that checks no component.
    pub const ANY: Self = Self {
        damage: None,
        banner_patterns: None,
        item_name_translate: None,
    };

    /// Whether no component check was given.
    #[must_use]
    pub const fn is_any(&self) -> bool {
        self.damage.is_none()
            && self.banner_patterns.is_none()
            && self.item_name_translate.is_none()
    }
}

/// A check on an item stack.
///
/// Vanilla parity: `ItemPredicate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPredicate {
    /// The items accepted, `None` for any item.
    pub items: Option<RegistrySet>,
    /// How many the stack must hold.
    pub count: IntBounds,
    /// Enchantment checks from `predicates.minecraft:enchantments`.
    pub enchantments: &'static [EnchantmentPredicate],
    /// Whether `predicates.minecraft:jukebox_playable` was asked for.
    ///
    /// Vanilla's sub-predicate has an optional `song` field that vanilla data
    /// never fills in, so presence is the whole check.
    pub jukebox_playable: bool,
    /// Component checks from the `components` map.
    pub components: ItemComponentsPredicate,
}

impl ItemPredicate {
    /// A predicate that accepts every stack, empty ones included.
    pub const ANY: Self = Self {
        items: None,
        count: IntBounds::ANY,
        enchantments: &[],
        jukebox_playable: false,
        components: ItemComponentsPredicate::ANY,
    };
}

/// A check on where something is.
///
/// Vanilla parity: `LocationPredicate`.
#[derive(Debug, Clone, PartialEq)]
pub struct LocationPredicate {
    /// Bounds on the x coordinate.
    pub x: DoubleBounds,
    /// Bounds on the y coordinate.
    pub y: DoubleBounds,
    /// Bounds on the z coordinate.
    pub z: DoubleBounds,
    /// The biomes accepted, `None` for any biome.
    pub biomes: Option<RegistrySet>,
    /// The structures the position must be inside, `None` for no check.
    pub structures: Option<RegistrySet>,
    /// The dimension the position must be in.
    pub dimension: Option<Identifier>,
    /// A check on the block at the position.
    pub block: Option<BlockPredicate>,
    /// Whether the position must be over a campfire's smoke.
    ///
    /// Vanilla parity: `LocationPredicate.smokey`, which is
    /// `CampfireBlock.isSmokeyPos`.
    pub smokey: Option<bool>,
}

impl LocationPredicate {
    /// A predicate that accepts every position.
    pub const ANY: Self = Self {
        x: DoubleBounds::ANY,
        y: DoubleBounds::ANY,
        z: DoubleBounds::ANY,
        biomes: None,
        structures: None,
        dimension: None,
        block: None,
        smokey: None,
    };
}

/// A check on how far away something is.
///
/// Vanilla parity: `DistancePredicate`, measured from the trigger's origin.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DistancePredicate {
    /// Bounds on the x offset.
    pub x: DoubleBounds,
    /// Bounds on the y offset.
    pub y: DoubleBounds,
    /// Bounds on the z offset.
    pub z: DoubleBounds,
    /// Bounds on the distance ignoring y.
    pub horizontal: DoubleBounds,
    /// Bounds on the straight-line distance.
    pub absolute: DoubleBounds,
}

/// The boolean entity states a vanilla advancement checks.
///
/// Vanilla parity: `EntityFlagsPredicate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntityFlagsPredicate {
    /// Whether the entity must be burning.
    pub is_on_fire: Option<bool>,
    /// Whether the entity must be crouching.
    pub is_sneaking: Option<bool>,
    /// Whether the entity must be sprinting.
    pub is_sprinting: Option<bool>,
    /// Whether the entity must be swimming.
    pub is_swimming: Option<bool>,
    /// Whether the entity must be a baby.
    pub is_baby: Option<bool>,
    /// Whether the entity must be flying with elytra.
    pub is_flying: Option<bool>,
}

/// A check on what an entity is wearing or holding.
///
/// Vanilla parity: `EntityEquipmentPredicate`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntityEquipmentPredicate {
    /// The helmet slot.
    pub head: Option<ItemPredicate>,
    /// The chestplate slot.
    pub chest: Option<ItemPredicate>,
    /// The leggings slot.
    pub legs: Option<ItemPredicate>,
    /// The boots slot.
    pub feet: Option<ItemPredicate>,
    /// The main hand.
    pub mainhand: Option<ItemPredicate>,
    /// The off hand.
    pub offhand: Option<ItemPredicate>,
    /// The animal body slot, which is where wolf armor and horse armor sit.
    pub body: Option<ItemPredicate>,
}

/// An entity data component check, by component key and value key.
///
/// Vanilla parity: the `minecraft:components` sub-predicate. Vanilla data only
/// ever compares a registry-keyed variant (`minecraft:cat/variant` and
/// friends), so the value is stored as the identifier it must equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityComponentMatch {
    /// The data component's key, such as `minecraft:wolf/variant`.
    pub component: &'static str,
    /// The identifier the component's value must equal.
    pub value: Identifier,
}

/// A check on an entity.
///
/// Vanilla parity: `EntityPredicate`. In 26.2 this is a map of registered
/// sub-predicates rather than the flat record older versions used, so each
/// field below names the sub-predicate key it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityPredicate {
    /// `minecraft:entity_type`.
    pub entity_type: Option<RegistrySet>,
    /// `minecraft:location`, checked at the entity's own position.
    pub location: Option<&'static LocationPredicate>,
    /// `minecraft:stepping_on`, checked at the block the entity stands on.
    pub stepping_on: Option<&'static LocationPredicate>,
    /// `minecraft:distance`, from the trigger's origin.
    pub distance: Option<DistancePredicate>,
    /// `minecraft:flags`.
    pub flags: Option<EntityFlagsPredicate>,
    /// `minecraft:equipment`.
    pub equipment: Option<&'static EntityEquipmentPredicate>,
    /// `minecraft:components`, all of which must match.
    pub components: &'static [EntityComponentMatch],
    /// `minecraft:vehicle`, checked against the entity being ridden.
    pub vehicle: Option<&'static EntityPredicate>,
    /// `minecraft:passenger`, checked against every rider until one matches.
    pub passenger: Option<&'static EntityPredicate>,
    /// `minecraft:type_specific/player`'s `looking_at`.
    pub looking_at: Option<&'static EntityPredicate>,
    /// `minecraft:type_specific/lightning`'s `blocks_set_on_fire`.
    pub lightning_blocks_set_on_fire: Option<IntBounds>,
}

impl EntityPredicate {
    /// A predicate that accepts every entity.
    pub const ANY: Self = Self {
        entity_type: None,
        location: None,
        stepping_on: None,
        distance: None,
        flags: None,
        equipment: None,
        components: &[],
        vehicle: None,
        passenger: None,
        looking_at: None,
        lightning_blocks_set_on_fire: None,
    };
}

/// One term of a criterion's condition list.
///
/// Vanilla parity: the `LootItemCondition` subtypes an advancement's
/// `ContextAwarePredicate` is built from. Vanilla's `EntityPredicate.wrap`
/// always targets `LootContextParams.THIS_ENTITY`, and every `entity` field in
/// vanilla advancement data reads `"this"`, so the target is not modeled --
/// the subject is whatever the trigger handed to this predicate.
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionTerm {
    /// `minecraft:entity_properties` on the subject.
    EntityProperties(&'static EntityPredicate),
    /// `minecraft:location_check` at the subject's position plus an offset.
    LocationCheck {
        /// Block offset applied before the check.
        offset_x: i32,
        /// Block offset applied before the check.
        offset_y: i32,
        /// Block offset applied before the check.
        offset_z: i32,
        /// The position check itself.
        predicate: &'static LocationPredicate,
    },
    /// `minecraft:match_tool` on the stack the trigger supplied.
    MatchTool(&'static ItemPredicate),
    /// `minecraft:block_state_property` on the block the trigger supplied.
    BlockStateProperty {
        /// The block the state must belong to.
        block: Identifier,
        /// Properties that must hold, all of them.
        properties: &'static [StatePropertyMatch],
    },
    /// `minecraft:any_of`: at least one term must pass.
    AnyOf(&'static [ConditionTerm]),
    /// `minecraft:all_of`: every term must pass.
    AllOf(&'static [ConditionTerm]),
    /// `minecraft:inverted`: the term must fail.
    Inverted(&'static ConditionTerm),
}

/// A criterion's condition list: every term must pass.
///
/// Vanilla parity: `ContextAwarePredicate`, which is a `List<LootItemCondition>`
/// evaluated with `allMatch`. An empty list passes, which is what makes an
/// absent `player` field accept everyone.
pub type ContextAwarePredicate = &'static [ConditionTerm];

/// A tag check on a damage source.
///
/// Vanilla parity: one entry of `TagPredicate<DamageType>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageTypeTagMatch {
    /// The damage type tag.
    pub id: Identifier,
    /// Whether the source must carry it (`true`) or must not (`false`).
    pub expected: bool,
}

/// A check on how damage was delivered.
///
/// Vanilla parity: `DamageSourcePredicate`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageSourcePredicate {
    /// Tag checks on the damage type, all of them.
    pub tags: &'static [DamageTypeTagMatch],
    /// A check on the entity that dealt the blow directly, the arrow rather
    /// than the archer.
    pub direct_entity: Option<&'static EntityPredicate>,
    /// A check on the entity ultimately responsible, the archer.
    pub source_entity: Option<&'static EntityPredicate>,
}

impl DamageSourcePredicate {
    /// A predicate that accepts every damage source.
    pub const ANY: Self = Self {
        tags: &[],
        direct_entity: None,
        source_entity: None,
    };
}

/// A check on a damage event.
///
/// Vanilla parity: `DamagePredicate`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamagePredicate {
    /// Bounds on the damage before armor and effects.
    pub dealt: DoubleBounds,
    /// Bounds on the health actually lost.
    pub taken: DoubleBounds,
    /// Whether the hit had to be blocked.
    pub blocked: Option<bool>,
    /// A check on the damage source.
    pub source: Option<&'static DamageSourcePredicate>,
}

impl DamagePredicate {
    /// A predicate that accepts every damage event.
    pub const ANY: Self = Self {
        dealt: DoubleBounds::ANY,
        taken: DoubleBounds::ANY,
        blocked: None,
        source: None,
    };
}
