//! Evaluating the predicates an advancement criterion asks about.
//!
//! Vanilla parity: `net.minecraft.advancements.predicates` plus the
//! `LootItemCondition`s a `ContextAwarePredicate` is built from.
//! [`steel_registry::advancement::predicate`] holds the plain data; this is the
//! half that needs a world, a player and the entity a trigger fired for.
//!
//! Vanilla evaluates every one of these against a `LootContext` built by
//! `EntityPredicate.createContext` or by the trigger itself, so
//! [`PredicateContext`] carries exactly the `LootContextParams` an advancement
//! ever reads: `THIS_ENTITY`, `ORIGIN`, `BLOCK_STATE` and `TOOL`, plus the
//! player whose level and position vanilla takes them from.
//!
//! Anything Steel cannot answer fails closed. A predicate that silently passes
//! hands out an advancement vanilla gates behind a state nobody checked, which
//! is strictly worse than one that never fires -- every such gap is marked with
//! a `// Not implemented:` comment naming the vanilla check it stands in for.

use glam::DVec3;
use steel_registry::advancement::predicate::{
    BannerPatternLayer, BlockPredicate, ConditionTerm, ContextAwarePredicate, DamagePredicate,
    DamageSourcePredicate, DistancePredicate, EnchantmentPredicate, EntityComponentMatch,
    EntityEquipmentPredicate, EntityFlagsPredicate, EntityPredicate, ItemPredicate,
    LocationPredicate, RegistrySet, StatePropertyMatch,
};
use steel_registry::biome::BiomeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::components::BannerPatternLayers;
use steel_registry::data_components::vanilla_components::{
    BANNER_PATTERNS, DAMAGE, ITEM_NAME, JUKEBOX_PLAYABLE,
};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::equipment::EquipmentSlot;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::loot_table::LootWorldView as _;
use steel_registry::{REGISTRY, RegistryExt as _, TaggedRegistryExt as _};
use steel_utils::{BlockPos, BlockStateId, Downcast as _};
use text_components::TextComponent;
use text_components::content::Content;

use crate::behavior::blocks::building::campfire_block::is_smokey_pos;
use crate::entity::damage::DamageSource;
use crate::entity::entities::{CatEntity, LightningBoltEntity, WolfEntity};
use crate::entity::{Entity, LivingEntity, SharedEntity};
use crate::player::Player;

/// The seven equipment slots vanilla's `EntityEquipmentPredicate` reads, in the
/// order it reads them.
const EQUIPMENT_SLOTS: [EquipmentSlot; 7] = [
    EquipmentSlot::Head,
    EquipmentSlot::Chest,
    EquipmentSlot::Legs,
    EquipmentSlot::Feet,
    EquipmentSlot::Body,
    EquipmentSlot::MainHand,
    EquipmentSlot::OffHand,
];

/// Vanilla parity: `Mth.floor(double)`, which is how `BlockPos.containing`
/// turns a predicate's raw coordinates into the block that contains them.
///
/// Flooring rather than truncating is what puts `-0.5` in the block at `-1`.
const fn block_coord(value: f64) -> i32 {
    value.floor() as i32
}

/// Vanilla parity: what `LootContextParams.THIS_ENTITY` resolves to.
///
/// [`Subject::None`] is vanilla's null, which fails every entity predicate --
/// `EntityPredicate.matches` opens on `entity == null ? false : ...`, so even an
/// empty predicate rejects it.
pub enum Subject<'a> {
    /// No subject at all, vanilla's null `THIS_ENTITY`.
    None,
    /// The player the trigger fired for.
    Player,
    /// Some other entity the trigger handed over.
    Entity(&'a SharedEntity),
}

/// Vanilla parity: the `LootContext` an advancement predicate is evaluated with.
pub struct PredicateContext<'a> {
    /// The player the trigger fired for. Vanilla takes the level and, for most
    /// triggers, the origin from them.
    pub player: &'a Player,
    /// Vanilla's `LootContextParams.ORIGIN`. The player's own position for most
    /// triggers; the block center for the location triggers.
    pub origin: DVec3,
    /// Vanilla's `LootContextParams.THIS_ENTITY`.
    pub subject: Subject<'a>,
    /// Vanilla's `LootContextParams.BLOCK_STATE`, set by the location triggers.
    pub block_state: Option<BlockStateId>,
    /// Vanilla's `LootContextParams.TOOL`, set by the location triggers.
    pub tool: Option<&'a ItemStack>,
}

impl PredicateContext<'_> {
    /// The entity a [`Subject`] names, `None` for vanilla's null.
    fn subject_entity<'s>(&'s self, subject: &'s Subject<'_>) -> Option<&'s dyn Entity> {
        match subject {
            Subject::None => None,
            Subject::Player => Some(self.player as &dyn Entity),
            Subject::Entity(entity) => Some(entity.as_ref()),
        }
    }

    /// Vanilla parity: `ContextAwarePredicate.matches`.
    ///
    /// Every term must pass; an empty list passes, which is what makes an absent
    /// `player` field on a criterion accept everyone.
    #[must_use]
    pub fn matches_conditions(&self, terms: ContextAwarePredicate) -> bool {
        terms.iter().all(|term| self.matches_condition(term))
    }

    /// Vanilla parity: one `LootItemCondition.test`.
    fn matches_condition(&self, term: &ConditionTerm) -> bool {
        match term {
            // Vanilla `LootItemEntityPropertyCondition.test`, whose entity
            // target is always `THIS` in advancement data.
            ConditionTerm::EntityProperties(predicate) => {
                self.matches_entity(predicate, &self.subject)
            }
            // Vanilla `LocationCheck.test`, which adds the block offset to the
            // raw `ORIGIN` doubles before flooring.
            ConditionTerm::LocationCheck {
                offset_x,
                offset_y,
                offset_z,
                predicate,
            } => self.matches_location(
                predicate,
                self.origin.x + f64::from(*offset_x),
                self.origin.y + f64::from(*offset_y),
                self.origin.z + f64::from(*offset_z),
            ),
            // Vanilla `MatchTool.test` is `tool != null && predicate.test(tool)`:
            // a missing TOOL fails outright, even for an empty predicate.
            ConditionTerm::MatchTool(predicate) => {
                self.tool.is_some_and(|tool| item_matches(predicate, tool))
            }
            // Vanilla `LootItemBlockStatePropertyCondition.test`: no BLOCK_STATE
            // fails, then the state's block and its properties must both match.
            ConditionTerm::BlockStateProperty { block, properties } => {
                self.block_state.is_some_and(|state| {
                    state.get_block().key == *block && state_properties_match(properties, state)
                })
            }
            ConditionTerm::AnyOf(terms) => terms.iter().any(|term| self.matches_condition(term)),
            ConditionTerm::AllOf(terms) => terms.iter().all(|term| self.matches_condition(term)),
            ConditionTerm::Inverted(term) => !self.matches_condition(term),
        }
    }

    /// Vanilla parity: `EntityPredicate.matches(level, position, entity)`.
    #[must_use]
    pub fn matches_entity(&self, predicate: &EntityPredicate, subject: &Subject<'_>) -> bool {
        self.entity_matches(predicate, self.subject_entity(subject))
    }

    /// The body of [`Self::matches_entity`], on a resolved entity.
    ///
    /// Vanilla's sub-predicate map is a pure conjunction, and a null subject
    /// fails all of it -- `EntityPredicate.matches` never even reaches the
    /// combined part.
    fn entity_matches(&self, predicate: &EntityPredicate, entity: Option<&dyn Entity>) -> bool {
        let Some(entity) = entity else {
            return false;
        };

        if let Some(types) = &predicate.entity_type
            && !entity_type_in_set(types, entity.entity_type())
        {
            return false;
        }

        let position = entity.position();
        if let Some(location) = predicate.location
            && !self.matches_location(location, position.x, position.y, position.z)
        {
            return false;
        }

        if let Some(stepping_on) = predicate.stepping_on
            && !self.matches_stepping_on(stepping_on, entity)
        {
            return false;
        }

        if let Some(distance) = &predicate.distance
            && !distance_matches(distance, self.origin, position)
        {
            return false;
        }

        if let Some(flags) = &predicate.flags
            && !flags_match(*flags, entity)
        {
            return false;
        }

        if let Some(equipment) = predicate.equipment
            && !equipment_matches(equipment, entity)
        {
            return false;
        }

        if !predicate
            .components
            .iter()
            .all(|component| entity_component_matches(component, entity))
        {
            return false;
        }

        self.entity_relations_match(predicate, entity)
    }

    /// The half of [`Self::entity_matches`] that walks off the subject: its
    /// vehicle, its passengers, what it is looking at and, for a bolt, how many
    /// blocks it lit.
    fn entity_relations_match(&self, predicate: &EntityPredicate, entity: &dyn Entity) -> bool {
        // Vanilla `VehiclePredicate` feeds `entity.getVehicle()` straight into
        // `EntityPredicate.matches`, so no vehicle is a null subject: false.
        if let Some(vehicle) = predicate.vehicle {
            let Some(ridden) = entity.vehicle() else {
                return false;
            };
            if !self.entity_matches(vehicle, Some(ridden.as_ref())) {
                return false;
            }
        }

        // Vanilla `PassengerPredicate` returns on the first direct passenger
        // that matches, and false when there are none.
        if let Some(passenger) = predicate.passenger
            && !entity
                .passengers()
                .iter()
                .any(|rider| self.entity_matches(passenger, Some(rider.as_ref())))
        {
            return false;
        }

        // Not implemented: `PlayerPredicate.lookingAt`, which needs
        // `ProjectileUtil.getEntityHitResult` along 100 blocks of the player's
        // view vector plus `ServerPlayer.hasLineOfSight`. Steel's equivalent
        // (`entity::projectile::get_entity_hit_result`) is private to that
        // module, so this cannot be answered from here and must not pass.
        if predicate.looking_at.is_some() {
            return false;
        }

        if let Some(bounds) = predicate.lightning_blocks_set_on_fire {
            // Vanilla `LightningBoltPredicate` rejects anything that is not a
            // `LightningBolt` before it looks at the count.
            let Some(bolt) = entity.downcast_ref::<LightningBoltEntity>() else {
                return false;
            };
            if !bounds.matches(bolt.blocks_set_on_fire()) {
                return false;
            }
        }

        true
    }

    /// Vanilla parity: `SteppingOnPredicate.matches`.
    ///
    /// The position is `Vec3.atCenterOf(entity.getOnPos())`. Vanilla's
    /// `getOnPos()` is `getOnPos(1.0E-5F)`, which is exactly the offset Steel's
    /// [`Entity::on_pos`] treats as "the supporting block itself".
    fn matches_stepping_on(&self, predicate: &LocationPredicate, entity: &dyn Entity) -> bool {
        if !entity.on_ground() {
            return false;
        }
        let Some(pos) = entity.on_pos(1.0e-5) else {
            return false;
        };
        self.matches_location(
            predicate,
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        )
    }

    /// Vanilla parity: `LocationPredicate.matches(level, x, y, z)`.
    ///
    /// The coordinate bounds are checked on the raw doubles, before anything is
    /// floored into a block position. `biomes`, `structures` and `smokey`
    /// additionally require the chunk to be loaded and are false when it is not.
    #[must_use]
    pub fn matches_location(&self, predicate: &LocationPredicate, x: f64, y: f64, z: f64) -> bool {
        if !predicate.x.matches(x) || !predicate.y.matches(y) || !predicate.z.matches(z) {
            return false;
        }

        let needs_level = predicate.dimension.is_some()
            || predicate.biomes.is_some()
            || predicate.structures.is_some()
            || predicate.smokey.is_some()
            || predicate.block.is_some();
        if !needs_level {
            return true;
        }

        // Vanilla always has a `ServerLevel` here; a player without one cannot
        // answer any of the remaining keys, so they all fail.
        let Some(level) = self.player.level() else {
            return false;
        };

        if let Some(dimension) = &predicate.dimension
            && level.key != *dimension
        {
            return false;
        }

        // Not implemented: `level.structureManager().getStructureWithPieceAt`.
        // Steel has no structure-at-position lookup at all, so a `structures`
        // check cannot be answered and must not pass.
        if predicate.structures.is_some() {
            return false;
        }

        let pos = BlockPos::new(block_coord(x), block_coord(y), block_coord(z));
        // `loaded_block_state` is `None` for exactly the positions vanilla's
        // `level.isLoaded(pos)` rejects.
        let loaded_state = level.loaded_block_state(pos.x(), pos.y(), pos.z());

        if let Some(biomes) = &predicate.biomes {
            let Some(biome) = level.loaded_biome(pos.x(), pos.y(), pos.z()) else {
                return false;
            };
            if !biome_in_set(biomes, biome) {
                return false;
            }
        }

        if let Some(smokey) = predicate.smokey
            && (loaded_state.is_none() || smokey != is_smokey_pos(&level, pos))
        {
            return false;
        }

        if let Some(block) = &predicate.block {
            let Some(state) = loaded_state else {
                return false;
            };
            if !block_state_matches(block, state) {
                return false;
            }
        }

        true
    }

    /// Vanilla parity: `DamageSourcePredicate.matches(level, position, source)`.
    #[must_use]
    pub fn matches_damage_source(
        &self,
        predicate: &DamageSourcePredicate,
        source: &DamageSource,
    ) -> bool {
        // Vanilla `TagPredicate.matches` is `holder.is(tag) == expected`.
        if !predicate
            .tags
            .iter()
            .all(|tag| source.is(&tag.id) == tag.expected)
        {
            return false;
        }

        if let Some(direct) = predicate.direct_entity
            && !self.matches_entity_by_id(direct, source.direct_entity_id)
        {
            return false;
        }

        if let Some(causing) = predicate.source_entity
            && !self.matches_entity_by_id(causing, source.causing_entity_id)
        {
            return false;
        }

        true
    }

    /// Vanilla parity: `DamagePredicate.matches(player, source, dealt, taken, blocked)`.
    ///
    /// Vanilla's `source_entity` field is not modeled by
    /// [`steel_registry::advancement::predicate::DamagePredicate`] because no
    /// vanilla advancement uses it; the nested `type.source_entity` is what the
    /// data reaches for instead.
    #[must_use]
    pub fn matches_damage(
        &self,
        predicate: &DamagePredicate,
        source: &DamageSource,
        dealt: f32,
        taken: f32,
        blocked: bool,
    ) -> bool {
        if !predicate.dealt.matches(f64::from(dealt)) || !predicate.taken.matches(f64::from(taken))
        {
            return false;
        }
        if let Some(expected) = predicate.blocked
            && expected != blocked
        {
            return false;
        }
        predicate
            .source
            .is_none_or(|source_predicate| self.matches_damage_source(source_predicate, source))
    }

    /// Resolves a [`DamageSource`]'s stored entity id and tests it.
    ///
    /// Steel stores ids rather than references, so an id that no longer resolves
    /// is vanilla's null entity: it fails the predicate.
    fn matches_entity_by_id(&self, predicate: &EntityPredicate, id: Option<i32>) -> bool {
        let Some(id) = id else {
            return false;
        };
        let Some(level) = self.player.level() else {
            return false;
        };
        let Some(entity) = level.get_entity_by_id(id) else {
            return false;
        };
        self.entity_matches(predicate, Some(entity.as_ref()))
    }
}

/// Vanilla parity: `EntityFlagsPredicate.matches`.
///
/// Note the asymmetry vanilla actually has: `is_flying` reads false for a
/// non-living entity and is then *compared*, so `is_flying: false` accepts an
/// arrow -- while `is_baby` is skipped entirely for a non-living entity and so
/// passes whatever it asked for.
fn flags_match(flags: EntityFlagsPredicate, entity: &dyn Entity) -> bool {
    let living = entity.as_living_entity();

    if let Some(expected) = flags.is_on_fire
        && entity.is_on_fire() != expected
    {
        return false;
    }
    if let Some(expected) = flags.is_sneaking
        && entity.is_crouching() != expected
    {
        return false;
    }
    // Vanilla reads `Entity.isSprinting()` off the shared entity flags; Steel
    // only tracks sprinting on living entities, so anything else is not
    // sprinting -- which is what the shared flag would say for it anyway.
    if let Some(expected) = flags.is_sprinting
        && living.is_some_and(LivingEntity::is_sprinting) != expected
    {
        return false;
    }
    if let Some(expected) = flags.is_swimming
        && entity.is_swimming() != expected
    {
        return false;
    }
    // Vanilla: `entity instanceof LivingEntity living && (living.isFallFlying()
    // || living instanceof Player player && player.getAbilities().flying)`.
    if let Some(expected) = flags.is_flying {
        let flying = living.is_some_and(LivingEntity::is_fall_flying) || entity.is_flying_player();
        if flying != expected {
            return false;
        }
    }
    // Vanilla skips the whole check for a non-living entity rather than
    // comparing against a default, so a non-living subject passes it.
    if let Some(expected) = flags.is_baby
        && let Some(living) = living
        && living.is_baby() != expected
    {
        return false;
    }

    true
}

/// Vanilla parity: `EntityEquipmentPredicate.matches`.
///
/// A non-living entity fails, even for an all-empty predicate -- vanilla's
/// `instanceof LivingEntity` check has an `else return false`.
fn equipment_matches(predicate: &EntityEquipmentPredicate, entity: &dyn Entity) -> bool {
    let Some(living) = entity.as_living_entity() else {
        return false;
    };

    let wanted = [
        &predicate.head,
        &predicate.chest,
        &predicate.legs,
        &predicate.feet,
        &predicate.body,
        &predicate.mainhand,
        &predicate.offhand,
    ];

    wanted.iter().zip(EQUIPMENT_SLOTS).all(|(item, slot)| {
        item.as_ref()
            .is_none_or(|item| item_matches(item, &living.get_item_by_slot(slot)))
    })
}

/// Vanilla parity: one entry of the `minecraft:components` entity sub-predicate.
///
/// Vanilla compares the entity's data component against the value the predicate
/// carries; Steel reaches the three variants vanilla advancement data actually
/// asks for and fails closed on anything else.
fn entity_component_matches(component: &EntityComponentMatch, entity: &dyn Entity) -> bool {
    match component.component {
        "minecraft:cat/variant" => entity
            .downcast_ref::<CatEntity>()
            .is_some_and(|cat| cat.variant().key == component.value),
        "minecraft:wolf/variant" => entity
            .downcast_ref::<WolfEntity>()
            .is_some_and(|wolf| wolf.variant().key == component.value),
        "minecraft:frog/variant" => entity
            .as_living_entity()
            .and_then(LivingEntity::frog_loot_variant)
            .is_some_and(|variant| *variant == component.value),
        // Not implemented: every other entity data component. Vanilla's
        // `EntityPartialComponentsPredicate` is open-ended and Steel has no
        // entity-wide component map to compare against, so an unmodeled key
        // must fail rather than pass unchecked.
        _ => false,
    }
}

/// Vanilla parity: `DistancePredicate.matches`.
///
/// Vanilla narrows every delta to `float` before comparing and sums the squares
/// in `float` too, so the rounding is part of the contract: a check written as
/// `absolute: {min: 30}` accepts positions a `double` computation would reject.
fn distance_matches(predicate: &DistancePredicate, origin: DVec3, target: DVec3) -> bool {
    let dx = (origin.x - target.x) as f32;
    let dy = (origin.y - target.y) as f32;
    let dz = (origin.z - target.z) as f32;

    if !predicate.x.matches(f64::from(dx.abs()))
        || !predicate.y.matches(f64::from(dy.abs()))
        || !predicate.z.matches(f64::from(dz.abs()))
    {
        return false;
    }

    predicate
        .horizontal
        .matches_sqr(f64::from(dx * dx + dz * dz))
        && predicate
            .absolute
            .matches_sqr(f64::from(dx * dx + dy * dy + dz * dz))
}

/// Whether `item` is in the holder set `set` names.
fn item_in_set(set: &RegistrySet, item: ItemRef) -> bool {
    match set {
        RegistrySet::Tag(tag) => item.has_tag(tag),
        RegistrySet::Entries(_) => set.contains_key(&item.key),
    }
}

/// Whether `block` is in the holder set `set` names.
fn block_in_set(set: &RegistrySet, block: BlockRef) -> bool {
    match set {
        RegistrySet::Tag(tag) => block.has_tag(tag),
        RegistrySet::Entries(_) => set.contains_key(&block.key),
    }
}

/// Whether `biome` is in the holder set `set` names.
fn biome_in_set(set: &RegistrySet, biome: BiomeRef) -> bool {
    match set {
        RegistrySet::Tag(tag) => biome.has_tag(tag),
        RegistrySet::Entries(_) => set.contains_key(&biome.key),
    }
}

/// Whether `entity_type` is in the holder set `set` names.
fn entity_type_in_set(set: &RegistrySet, entity_type: EntityTypeRef) -> bool {
    match set {
        RegistrySet::Tag(tag) => REGISTRY.entity_types.is_in_tag(entity_type, tag),
        RegistrySet::Entries(_) => set.contains_key(&entity_type.key),
    }
}

/// Vanilla parity: `ItemPredicate.test`.
///
/// An empty stack reports as `minecraft:air` with count `0`, so an
/// [`ItemPredicate`] that asks nothing matches [`ItemStack::empty`] -- which is
/// what lets an absent `item` field on a criterion accept a bare hand.
#[must_use]
pub fn item_matches(predicate: &ItemPredicate, stack: &ItemStack) -> bool {
    if let Some(items) = &predicate.items
        && !item_in_set(items, stack.item())
    {
        return false;
    }
    if !predicate.count.matches(stack.count()) {
        return false;
    }
    item_components_match(predicate, stack)
}

/// Vanilla parity: `DataComponentMatchers.test`, narrowed to the checks vanilla
/// advancement data uses.
///
/// The exact half compares against the stack's *effective* component value --
/// its patch, or the item prototype when the patch says nothing -- and an
/// absent component fails, because `Objects.equals(expected, null)` is false.
fn item_components_match(predicate: &ItemPredicate, stack: &ItemStack) -> bool {
    let components = &predicate.components;

    if let Some(damage) = components.damage
        && stack.get(DAMAGE).copied() != Some(damage)
    {
        return false;
    }

    if let Some(layers) = components.banner_patterns {
        let Some(actual) = stack.get(BANNER_PATTERNS) else {
            return false;
        };
        if !banner_patterns_equal(layers, actual) {
            return false;
        }
    }

    if let Some(key) = components.item_name_translate {
        let Some(name) = stack.get(ITEM_NAME) else {
            return false;
        };
        if !is_translatable_with_key(name, key) {
            return false;
        }
    }

    // Vanilla `JukeboxPlayablePredicate` with no `song` is a bare presence
    // check on the component, which is the only shape vanilla data uses.
    if predicate.jukebox_playable && !stack.has(JUKEBOX_PLAYABLE) {
        return false;
    }

    predicate
        .enchantments
        .iter()
        .all(|enchantment| enchantment_contained_in(enchantment, stack))
}

/// Vanilla parity: comparing a `minecraft:banner_patterns` component for exact
/// equality, layer by layer and in order.
fn banner_patterns_equal(expected: &[BannerPatternLayer], actual: &BannerPatternLayers) -> bool {
    let actual = actual.layers();
    actual.len() == expected.len()
        && expected.iter().zip(actual).all(|(want, have)| {
            have.color().serialized_name() == want.color
                && have
                    .pattern()
                    .as_reference()
                    .is_some_and(|pattern| pattern.key == want.pattern)
        })
}

/// Whether `component` is a translatable component with key `key`.
///
/// Vanilla compares the whole `minecraft:item_name` value, style included; the
/// generator only ever sees a bare translate key, so the key is the whole check
/// Steel can express. The one vanilla use (the ominous banner's name) is unique
/// enough for that to be the same answer.
fn is_translatable_with_key(component: &TextComponent, key: &str) -> bool {
    matches!(&component.content, Content::Translate(message) if message.key.as_ref() == key)
}

/// Vanilla parity: `EnchantmentPredicate.containedIn`.
fn enchantment_contained_in(predicate: &EnchantmentPredicate, stack: &ItemStack) -> bool {
    let Some(enchantments) = stack.get_enchantments() else {
        return false;
    };

    match &predicate.enchantments {
        // Vanilla `matchesEnchantment`: an enchantment at level zero is absent,
        // and any other level has to fall inside the bounds.
        Some(RegistrySet::Entries(keys)) => keys.iter().any(|key| {
            let level = enchantments.get_level(key);
            level != 0 && predicate.levels.matches(level as i32)
        }),
        Some(RegistrySet::Tag(tag)) => enchantments.iter().any(|(key, level)| {
            *level != 0
                && predicate.levels.matches(*level as i32)
                && REGISTRY
                    .enchantments
                    .by_key(key)
                    .is_some_and(|enchantment| REGISTRY.enchantments.is_in_tag(enchantment, tag))
        }),
        // No `enchantments` field: bounded levels ask whether anything on the
        // stack sits inside them, and unbounded ones ask only for any
        // enchantment at all.
        None if predicate.levels.is_any() => !enchantments.is_empty(),
        None => enchantments
            .iter()
            .any(|(_, level)| predicate.levels.matches(*level as i32)),
    }
}

/// Vanilla parity: `BlockPredicate.matchesState`, the half that needs no world.
///
/// The `nbt` and `components` halves of vanilla's predicate are not modeled --
/// no vanilla advancement uses them, and the generator refuses one that does.
#[must_use]
pub fn block_state_matches(predicate: &BlockPredicate, state: BlockStateId) -> bool {
    if let Some(blocks) = &predicate.blocks
        && !block_in_set(blocks, state.get_block())
    {
        return false;
    }
    state_properties_match(predicate.state, state)
}

/// Vanilla parity: `StatePropertiesPredicate.matches`.
///
/// A property the block does not have fails: vanilla's `PropertyMatcher.match`
/// opens on `property != null`, so an unknown name is never vacuously true.
#[must_use]
pub fn state_properties_match(properties: &[StatePropertyMatch], state: BlockStateId) -> bool {
    properties
        .iter()
        .all(|property| state.get_property_str(property.name).as_deref() == Some(property.value))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use steel_registry::advancement::predicate::{
        ConditionTerm, IntBounds, ItemPredicate, RegistrySet, StatePropertyMatch,
    };
    use steel_registry::item_stack::ItemStack;
    use steel_registry::vanilla_item_tags::ItemTag;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};

    use super::{PredicateContext, Subject, item_matches, state_properties_match};
    use crate::player::Player;
    use crate::test_support::{TestPlayerBuilder, test_world};

    static ANY_TOOL: ItemPredicate = ItemPredicate::ANY;
    static MATCH_TOOL: ConditionTerm = ConditionTerm::MatchTool(&ANY_TOOL);
    static MATCH_TOOL_TERMS: &[ConditionTerm] = &[ConditionTerm::MatchTool(&ANY_TOOL)];
    static INVERTED_MATCH_TOOL_TERMS: &[ConditionTerm] = &[ConditionTerm::Inverted(&MATCH_TOOL)];
    static EMPTY_ANY_OF_TERMS: &[ConditionTerm] = &[ConditionTerm::AnyOf(&[])];
    static EMPTY_ALL_OF_TERMS: &[ConditionTerm] = &[ConditionTerm::AllOf(&[])];

    fn test_player() -> Arc<Player> {
        TestPlayerBuilder::new(Arc::clone(test_world()), "PredicateTester", 900).build()
    }

    fn context_with_tool<'a>(
        player: &'a Player,
        tool: Option<&'a ItemStack>,
    ) -> PredicateContext<'a> {
        PredicateContext {
            player,
            origin: DVec3::ZERO,
            subject: Subject::None,
            block_state: None,
            tool,
        }
    }

    #[test]
    fn item_predicate_tag_accepts_planks_and_rejects_stone() {
        init_vanilla_registry();
        let predicate = ItemPredicate {
            items: Some(RegistrySet::Tag(ItemTag::PLANKS)),
            ..ItemPredicate::ANY
        };

        assert!(item_matches(
            &predicate,
            &ItemStack::new(&vanilla_items::OAK_PLANKS)
        ));
        assert!(!item_matches(
            &predicate,
            &ItemStack::new(&vanilla_items::STONE)
        ));
    }

    #[test]
    fn empty_item_predicate_matches_the_empty_stack() {
        init_vanilla_registry();

        // Vanilla reports the empty stack as `minecraft:air` with count 0, so a
        // predicate that asks nothing accepts it.
        assert!(item_matches(&ItemPredicate::ANY, &ItemStack::empty()));

        // ...and one that asks for a single item does not, which is what makes
        // the assertion above mean something.
        let at_least_one = ItemPredicate {
            count: IntBounds {
                min: Some(1),
                max: None,
            },
            ..ItemPredicate::ANY
        };
        assert!(!item_matches(&at_least_one, &ItemStack::empty()));
    }

    #[test]
    fn item_predicate_count_bounds_are_inclusive() {
        init_vanilla_registry();
        let predicate = ItemPredicate {
            count: IntBounds {
                min: Some(2),
                max: Some(4),
            },
            ..ItemPredicate::ANY
        };
        let stack = |count| ItemStack::with_count(&vanilla_items::STONE, count);

        assert!(!item_matches(&predicate, &stack(1)));
        assert!(item_matches(&predicate, &stack(2)));
        assert!(item_matches(&predicate, &stack(4)));
        assert!(!item_matches(&predicate, &stack(5)));
    }

    #[test]
    fn state_properties_reject_a_property_the_block_does_not_have() {
        init_vanilla_registry();
        let slab = vanilla_blocks::OAK_SLAB.default_state();

        assert!(state_properties_match(
            &[StatePropertyMatch {
                name: "waterlogged",
                value: "false",
            }],
            slab
        ));
        assert!(!state_properties_match(
            &[StatePropertyMatch {
                name: "waterlogged",
                value: "true",
            }],
            slab
        ));

        // A slab has no `lit`; vanilla's `property != null` guard makes that
        // false rather than vacuously true.
        assert!(!state_properties_match(
            &[StatePropertyMatch {
                name: "lit",
                value: "false",
            }],
            slab
        ));
    }

    #[test]
    fn match_tool_without_a_tool_fails_and_inverts_to_true() {
        init_vanilla_registry();
        let player = test_player();
        let toolless = context_with_tool(&player, None);

        assert!(!toolless.matches_conditions(MATCH_TOOL_TERMS));
        assert!(toolless.matches_conditions(INVERTED_MATCH_TOOL_TERMS));

        // With a tool the very same empty predicate passes, so the failure
        // above is the missing TOOL and not the predicate itself.
        let stone = ItemStack::new(&vanilla_items::STONE);
        let armed = context_with_tool(&player, Some(&stone));
        assert!(armed.matches_conditions(MATCH_TOOL_TERMS));
        assert!(!armed.matches_conditions(INVERTED_MATCH_TOOL_TERMS));
    }

    #[test]
    fn empty_any_of_fails_and_empty_all_of_passes() {
        init_vanilla_registry();
        let player = test_player();
        let context = context_with_tool(&player, None);

        // Vanilla builds these out of `Stream.anyMatch` and `Stream.allMatch`,
        // which disagree about the empty list.
        assert!(!context.matches_conditions(EMPTY_ANY_OF_TERMS));
        assert!(context.matches_conditions(EMPTY_ALL_OF_TERMS));

        // An empty condition list passes too, which is what makes an absent
        // `player` field accept everyone.
        assert!(context.matches_conditions(&[]));
    }
}
