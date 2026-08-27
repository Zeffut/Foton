use text_components::TextComponent;

use super::{
    DyeColor, EquipmentSlotGroup, Identifier, InstrumentRef, ItemStack, LootCondition, LootContext,
    LootContextEntity, LootEntry, NumberProvider, REGISTRY, RngExt, TaggedRegistryExt, math_round,
};

/// Options for selecting enchantments - either a tag reference or explicit list.
#[derive(Debug, Clone)]
pub enum EnchantmentOptions {
    /// Reference to an enchantment tag (e.g., "`on_random_loot`").
    Tag(Identifier),
    /// Explicit list of enchantment IDs.
    List(&'static [Identifier]),
}

/// Options for selecting a potion - either a tag reference or explicit list.
///
/// Vanilla parity: the `HolderSet<Potion>` of `SetRandomPotionFunction`.
#[derive(Debug, Clone)]
pub enum PotionOptions {
    /// Reference to a potion tag (e.g., "`tradeable`").
    Tag(Identifier),
    /// Explicit list of potion IDs.
    List(&'static [Identifier]),
}

/// Options for selecting an instrument from a registry tag or explicit list.
#[derive(Debug, Clone)]
pub enum InstrumentOptions {
    Tag(Identifier),
    Direct(&'static [InstrumentRef]),
}

impl InstrumentOptions {
    fn get_random<R: rand::Rng>(&self, rng: &mut R) -> Option<InstrumentRef> {
        match self {
            Self::Tag(tag) => {
                let instruments = REGISTRY.instruments.get_tag(tag)?;
                (!instruments.is_empty()).then(|| {
                    let index = rng.random_range(0..instruments.len());
                    instruments[index]
                })
            }
            Self::Direct(instruments) => (!instruments.is_empty()).then(|| {
                let index = rng.random_range(0..instruments.len());
                instruments[index]
            }),
        }
    }
}

/// A function with optional conditions.
#[derive(Debug, Clone)]
pub struct ConditionalLootFunction {
    pub function: LootFunction,
    pub conditions: &'static [LootCondition],
}

/// A function that modifies loot items.
#[derive(Debug, Clone)]
pub enum LootFunction {
    /// Set the count of the item.
    SetCount { count: NumberProvider, add: bool },
    /// Apply explosion decay - each item has 1/radius chance to survive.
    ExplosionDecay,
    /// Apply bonus count based on enchantment level.
    ApplyBonus {
        enchantment: Identifier,
        formula: BonusFormula,
    },
    /// Increase count based on enchantment (like looting).
    EnchantedCountIncrease {
        enchantment: Identifier,
        count: NumberProvider,
        limit: i32,
    },
    /// Limit the count to a range.
    LimitCount { min: Option<i32>, max: Option<i32> },
    /// Set the damage of the item (0.0 = broken, 1.0 = full durability).
    SetDamage { damage: NumberProvider, add: bool },
    /// Enchant the item randomly with enchantments from options.
    EnchantRandomly {
        /// Absent means "every registered enchantment".
        options: Option<EnchantmentOptions>,
        /// Whether the drawn enchantment has to be one this item accepts.
        /// Vanilla default is `true`; a book ignores it either way.
        only_compatible: bool,
        /// Whether to bank the enchantment's cost in `ADDITIONAL_TRADE_COST`.
        include_additional_cost_component: bool,
    },
    /// Enchant the item as if using an enchanting table at the specified level.
    EnchantWithLevels {
        levels: NumberProvider,
        /// Absent means "every registered enchantment".
        options: Option<EnchantmentOptions>,
        /// Whether to bank the enchantment cost in `ADDITIONAL_TRADE_COST`.
        include_additional_cost_component: bool,
    },
    /// Copy components from the block entity to the item.
    CopyComponents {
        source: CopySource,
        include: &'static [Identifier],
    },
    /// Copy block state properties to the item.
    CopyState {
        block: Identifier,
        properties: &'static [&'static str],
    },
    /// Set components on the item.
    SetComponents { components: &'static str },
    /// Set custom NBT data on the item (merges with existing `custom_data`).
    SetCustomData {
        tag: fn() -> crate::data_components::CustomData,
    },
    /// Smelt the item (convert raw to cooked, ore to ingot, etc.).
    FurnaceSmelt { use_input_count: bool },
    /// Create an exploration map pointing to a structure.
    ExplorationMap {
        destination: Identifier,
        decoration: Identifier,
        zoom: i32,
        skip_existing_chunks: bool,
    },
    /// Set the custom name or item name of the item.
    SetName {
        name: fn() -> TextComponent,
        target: NameTarget,
    },
    /// Set the ominous bottle amplifier.
    SetOminousBottleAmplifier { amplifier: NumberProvider },
    /// Set the potion type.
    SetPotion { id: Identifier },
    /// Put a potion drawn at random into the item.
    ///
    /// Vanilla parity: `SetRandomPotionFunction`. Absent options mean the whole
    /// potion registry.
    SetRandomPotion { options: Option<PotionOptions> },
    /// Dye the item with `number_of_dyes` dyes drawn at random.
    ///
    /// Vanilla parity: `SetRandomDyesFunction`.
    SetRandomDyes { number_of_dyes: NumberProvider },
    /// Set the suspicious stew effects.
    SetStewEffect { effects: &'static [StewEffect] },
    /// Set the instrument for goat horns.
    SetInstrument { options: InstrumentOptions },
    /// Set enchantments on the item.
    SetEnchantments {
        enchantments: &'static [(Identifier, NumberProvider)],
        add: bool,
    },
    /// Change the item type entirely.
    SetItem { item: Identifier },
    /// Copy name from source entity/block to item.
    CopyName { source: CopySource },
    /// Add lore lines to the item.
    SetLore {
        lore: &'static [&'static str],
        mode: ListOperation,
    },
    /// Set container inventory contents.
    SetContents {
        entries: &'static [LootEntry],
        component_type: Identifier,
    },
    /// Modify existing container contents.
    ModifyContents {
        modifier: &'static [ConditionalLootFunction],
        component_type: Identifier,
    },
    /// Set container's loot table reference.
    SetLootTable {
        loot_table: Identifier,
        seed: Option<i64>,
    },
    /// Set attribute modifiers on the item.
    SetAttributes {
        modifiers: &'static [AttributeModifier],
        replace: bool,
    },
    /// Fill player head with texture from entity.
    FillPlayerHead { entity: LootContextEntity },
    /// Copy NBT/custom data from source.
    CopyCustomData {
        source: CopySource,
        operations: &'static [CopyDataOperation],
    },
    /// Set banner pattern layers.
    SetBannerPattern {
        patterns: &'static [BannerPattern],
        append: bool,
    },
    /// Set firework rocket properties.
    SetFireworks {
        explosions: Option<&'static [FireworkExplosion]>,
        flight_duration: Option<i32>,
    },
    /// Set firework star explosion properties.
    SetFireworkExplosion { explosion: FireworkExplosion },
    /// Set book cover (title/author for written books).
    SetBookCover {
        title: Option<&'static str>,
        author: Option<&'static str>,
        generation: Option<i32>,
    },
    /// Set written book page contents.
    SetWrittenBookPages {
        pages: &'static [&'static str],
        mode: ListOperation,
    },
    /// Set writable book page contents.
    SetWritableBookPages {
        pages: &'static [&'static str],
        mode: ListOperation,
    },
    /// Toggle tooltip visibility.
    ToggleTooltips {
        toggles: &'static [(Identifier, bool)],
    },
    /// Discard/delete the item entirely.
    Discard,
    /// Reference to a named function in the registry.
    Reference(Identifier),
    /// Apply multiple functions in sequence.
    Sequence {
        functions: &'static [ConditionalLootFunction],
    },
    /// Branch on whether the item matches a predicate.
    ///
    /// Vanilla parity: `FilteredFunction`, which since 26.2 carries an
    /// `on_pass` *and* an `on_fail` branch rather than a single modifier. The
    /// `on_fail` half is what makes a villager's enchanted-gear trade vanish
    /// when the enchantment did not take, instead of selling plain gear.
    Filtered {
        item_filter: ItemFilter,
        on_pass: Option<&'static ConditionalLootFunction>,
        on_fail: Option<&'static ConditionalLootFunction>,
    },
}

/// The items an [`ItemFilter`] accepts.
#[derive(Debug, Clone)]
pub enum ItemFilterItems {
    /// Reference to an item tag.
    Tag(Identifier),
    /// Explicit list of item IDs.
    List(&'static [Identifier]),
}

impl ItemFilterItems {
    fn test(&self, item: &ItemStack) -> bool {
        match self {
            Self::Tag(tag) => REGISTRY
                .items
                .get_tag(tag)
                .is_some_and(|items| items.iter().any(|candidate| item.is(candidate))),
            Self::List(ids) => ids.contains(&item.item.key),
        }
    }
}

/// One entry of an [`ItemFilter`]'s `predicates` map.
///
/// Vanilla parity: the values of `DataComponentMatchers.partial`. Only the
/// shapes the vanilla data actually uses are modeled; the build script fails
/// on anything else rather than letting an unchecked predicate pass.
#[derive(Debug, Clone)]
pub enum ItemComponentPredicate {
    /// Vanilla parity: `DataComponentPredicate.AnyValueType` -- the named
    /// component is present on the stack, whatever its value.
    Present(Identifier),
    /// Vanilla parity: `EnchantmentsPredicate.Enchantments` built from `[{}]`,
    /// which passes when the stack carries at least one enchantment.
    AnyEnchantment,
    /// Vanilla parity: `EnchantmentsPredicate.StoredEnchantments` built from `[{}]`.
    AnyStoredEnchantment,
}

/// Vanilla parity: the `ItemPredicate` a `minecraft:filtered` function tests.
#[derive(Debug, Clone)]
pub struct ItemFilter {
    /// The accepted items. `None` accepts any item.
    pub items: Option<ItemFilterItems>,
    /// Component predicates that must all pass.
    pub predicates: &'static [ItemComponentPredicate],
}

impl ItemFilter {
    /// Vanilla parity: `ItemPredicate.test`.
    #[must_use]
    pub fn test(&self, item: &ItemStack) -> bool {
        if item.is_empty() {
            return false;
        }
        if let Some(items) = &self.items
            && !items.test(item)
        {
            return false;
        }

        self.predicates.iter().all(|predicate| match predicate {
            ItemComponentPredicate::Present(component) => item.has_component(component),
            ItemComponentPredicate::AnyEnchantment => item
                .get(crate::data_components::vanilla_components::ENCHANTMENTS)
                .is_some_and(|enchantments| !enchantments.is_empty()),
            ItemComponentPredicate::AnyStoredEnchantment => item
                .get(crate::data_components::vanilla_components::STORED_ENCHANTMENTS)
                .is_some_and(|enchantments| !enchantments.is_empty()),
        })
    }
}

/// Operation mode for list modifications (lore, book pages).
#[derive(Debug, Clone, Copy)]
pub enum ListOperation {
    /// Replace all existing entries.
    ReplaceAll,
    /// Replace a section of entries.
    ReplaceSection { offset: i32, size: Option<i32> },
    /// Insert before existing entries.
    InsertBefore { offset: i32 },
    /// Insert after existing entries.
    InsertAfter { offset: i32 },
    /// Append to the end.
    Append,
}

/// An attribute modifier for `SetAttributes` function.
#[derive(Debug, Clone)]
pub struct AttributeModifier {
    pub attribute: Identifier,
    pub operation: AttributeOperation,
    pub amount: NumberProvider,
    pub id: Identifier,
    pub slot: EquipmentSlotGroup,
}

/// Attribute modifier operation type.
#[expect(clippy::enum_variant_names, reason = "matches Vanilla naming")]
#[derive(Debug, Clone, Copy)]
pub enum AttributeOperation {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}

/// Copy data operation for `CopyCustomData`.
#[derive(Debug, Clone)]
pub struct CopyDataOperation {
    pub source_path: &'static str,
    pub target_path: &'static str,
    pub op: CopyDataOp,
}

/// Operation type for data copying.
#[derive(Debug, Clone, Copy)]
pub enum CopyDataOp {
    Replace,
    Append,
    Merge,
}

/// A banner pattern layer.
#[derive(Debug, Clone)]
pub struct BannerPattern {
    pub pattern: Identifier,
    pub color: DyeColor,
}

/// A firework explosion definition.
#[derive(Debug, Clone)]
pub struct FireworkExplosion {
    pub shape: FireworkShape,
    pub colors: &'static [i32],
    pub fade_colors: &'static [i32],
    pub has_trail: bool,
    pub has_twinkle: bool,
}

/// Firework explosion shape.
#[derive(Debug, Clone, Copy)]
pub enum FireworkShape {
    SmallBall,
    LargeBall,
    Star,
    Creeper,
    Burst,
}

/// Formula types for `apply_bonus` function.
#[derive(Debug, Clone, Copy)]
pub enum BonusFormula {
    /// Ore drops formula: count * (max(0, random(0..level+2) - 1) + 1)
    OreDrops,
    /// Uniform bonus: count + random(0..bonusMultiplier * level + 1)
    UniformBonusCount { bonus_multiplier: i32 },
    /// Binomial with bonus count: for each of (level + extra) trials, probability p to add 1
    BinomialWithBonusCount { extra: i32, probability: f32 },
}

/// Source for copying components.
#[derive(Debug, Clone, Copy)]
pub enum CopySource {
    BlockEntity,
    This,
    Attacker,
    DirectAttacker,
}

/// Target for `set_name` function.
///
/// Vanilla parity: `SetNameFunction.Target`, whose `component()` picks between
/// the two name components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameTarget {
    CustomName,
    ItemName,
}

/// A stew effect for suspicious stew.
#[derive(Debug, Clone)]
pub struct StewEffect {
    pub effect_type: Identifier,
    pub duration: NumberProvider,
}

impl LootFunction {
    /// Apply this function to modify the item stack.
    ///
    /// This modifies the item in place. Functions can change:
    /// - Count (`SetCount`, `ExplosionDecay`, `ApplyBonus`, etc.)
    /// - Damage/durability (`SetDamage`)
    /// - Enchantments (`EnchantRandomly`, `EnchantWithLevels`, `SetEnchantments`)
    /// - Components/NBT (`CopyComponents`, `SetComponents`, `CopyState`)
    /// - Item type (`FurnaceSmelt`)
    /// - And more...
    pub fn apply<R: rand::Rng>(&self, item: &mut ItemStack, ctx: &mut LootContext<'_, R>) {
        match self {
            LootFunction::SetCount {
                count: provider,
                add,
            } => {
                let value = provider.get_int(ctx.rng);
                if *add {
                    item.count += value;
                } else {
                    item.count = value;
                }
            }
            LootFunction::ExplosionDecay => {
                if let Some(radius) = ctx.explosion_radius {
                    // Each item has 1/radius chance to survive
                    let probability = 1.0 / radius;
                    let mut result_count = 0;
                    for _ in 0..item.count {
                        if ctx.rng.random::<f32>() <= probability {
                            result_count += 1;
                        }
                    }
                    item.count = result_count;
                }
            }
            LootFunction::ApplyBonus {
                enchantment,
                formula,
            } => {
                // Vanilla guards the whole body on the TOOL parameter being
                // present. Without it the count is untouched and, just as
                // importantly, no randomness is drawn.
                let Some(tool) = ctx.tool else {
                    return;
                };
                let level = tool.get_enchantment_level(enchantment);
                item.count = formula.apply(item.count, level, ctx.rng);
            }
            LootFunction::EnchantedCountIncrease {
                enchantment,
                count: provider,
                limit,
            } => {
                // Vanilla reads ATTACKING_ENTITY, not TOOL: looting sits on the
                // killer's weapon, and an entity loot roll never sets TOOL.
                let level =
                    ctx.get_entity_enchantment_level(LootContextEntity::Killer, enchantment);
                if level == 0 {
                    return;
                }
                let addition = provider.get_simple(ctx.rng) * level as f32;
                item.count += math_round(addition);
                // `limit` clamps the resulting stack (vanilla `ItemStack.limitSize`),
                // not the bonus that was just added.
                if *limit > 0 {
                    item.count = item.count.min(*limit);
                }
            }
            LootFunction::LimitCount { min, max } => {
                if let Some(min_val) = min {
                    item.count = item.count.max(*min_val);
                }
                if let Some(max_val) = max {
                    item.count = item.count.min(*max_val);
                }
            }
            LootFunction::SetDamage { damage, add } => {
                item.set_damage_fraction(damage.get_simple(ctx.rng), *add);
            }
            LootFunction::EnchantRandomly {
                options,
                only_compatible,
                include_additional_cost_component,
            } => {
                let Some(cost) = item.enchant_randomly(options.as_ref(), *only_compatible, ctx.rng)
                else {
                    return;
                };
                if *include_additional_cost_component && ctx.additional_cost_component_allowed {
                    item.set(
                        crate::data_components::vanilla_components::ADDITIONAL_TRADE_COST,
                        cost,
                    );
                }
            }
            LootFunction::EnchantWithLevels {
                levels,
                options,
                include_additional_cost_component,
            } => {
                let enchantment_cost = levels.get_int(ctx.rng);
                item.enchant_with_levels(enchantment_cost, options.as_ref(), ctx.rng);
                if *include_additional_cost_component
                    && ctx.additional_cost_component_allowed
                    && !item.is_empty()
                    && enchantment_cost > 0
                {
                    item.set(
                        crate::data_components::vanilla_components::ADDITIONAL_TRADE_COST,
                        enchantment_cost,
                    );
                }
            }
            LootFunction::CopyComponents { source, include } => {
                // TODO: Implement when block entity system is ready
                item.copy_components(*source, include, ctx);
            }
            LootFunction::CopyState { block, properties } => {
                // Vanilla's `block` only validates the property names when the
                // function is built; `CopyBlockState.run` never reads it.
                let _ = block;
                item.copy_block_state(properties, ctx);
            }
            LootFunction::SetComponents { components } => {
                // TODO: Implement component setting from JSON
                item.set_components_from_json(components);
            }
            LootFunction::SetCustomData { tag } => {
                item.set_custom_data(&tag());
            }
            LootFunction::FurnaceSmelt { use_input_count } => {
                item.apply_furnace_smelt(*use_input_count);
            }
            LootFunction::ExplorationMap {
                destination,
                decoration,
                zoom,
                skip_existing_chunks,
            } => {
                // TODO: Implement exploration map creation
                item.create_exploration_map(destination, decoration, *zoom, *skip_existing_chunks);
            }
            LootFunction::SetName { name, target } => {
                item.set_name(name(), *target);
            }
            LootFunction::SetOminousBottleAmplifier { amplifier } => {
                let amp = amplifier.get_int(ctx.rng).clamp(
                    crate::data_components::OminousBottleAmplifier::MIN_AMPLIFIER,
                    crate::data_components::OminousBottleAmplifier::MAX_AMPLIFIER,
                );
                item.set_ominous_bottle_amplifier(amp);
            }
            LootFunction::SetPotion { id } => {
                item.set_potion(id);
            }
            LootFunction::SetRandomPotion { options } => {
                item.set_random_potion(options.as_ref(), ctx.rng);
            }
            LootFunction::SetRandomDyes { number_of_dyes } => {
                item.set_random_dyes(number_of_dyes.get_int(ctx.rng), ctx.rng);
            }
            LootFunction::SetStewEffect { effects } => {
                item.set_stew_effects(effects, ctx.rng);
            }
            LootFunction::SetInstrument { options } => {
                if let Some(instrument) = options.get_random(ctx.rng) {
                    item.set(
                        crate::data_components::vanilla_components::INSTRUMENT,
                        crate::data_components::InstrumentComponent::new(
                            crate::RegistryHolder::reference(instrument),
                        ),
                    );
                }
            }
            LootFunction::SetEnchantments { enchantments, add } => {
                let resolved: Vec<(Identifier, u32)> = enchantments
                    .iter()
                    .map(|(key, provider)| (key.clone(), provider.get_int(ctx.rng).max(0) as u32))
                    .collect();
                item.set_enchantments(&resolved, *add);
            }
            LootFunction::SetItem { item: new_item } => {
                item.set_item(new_item);
            }
            LootFunction::CopyName { source } => {
                item.copy_name(*source, ctx);
            }
            LootFunction::SetLore { lore, mode } => {
                item.set_lore(lore, *mode);
            }
            LootFunction::SetContents {
                entries,
                component_type,
            } => {
                item.set_contents(entries, component_type, ctx);
            }
            LootFunction::ModifyContents {
                modifier,
                component_type,
            } => {
                item.modify_contents(modifier, component_type, ctx);
            }
            LootFunction::SetLootTable { loot_table, seed } => {
                item.set_loot_table(loot_table, *seed);
            }
            LootFunction::SetAttributes { modifiers, replace } => {
                item.set_attributes(modifiers, *replace, ctx.rng);
            }
            LootFunction::FillPlayerHead { entity } => {
                item.fill_player_head(*entity, ctx);
            }
            LootFunction::CopyCustomData { source, operations } => {
                item.copy_custom_data(*source, operations, ctx);
            }
            LootFunction::SetBannerPattern { patterns, append } => {
                item.set_banner_pattern(patterns, *append);
            }
            LootFunction::SetFireworks {
                explosions,
                flight_duration,
            } => {
                item.set_fireworks(*explosions, *flight_duration);
            }
            LootFunction::SetFireworkExplosion { explosion } => {
                item.set_firework_explosion(explosion);
            }
            LootFunction::SetBookCover {
                title,
                author,
                generation,
            } => {
                item.set_book_cover(*title, *author, *generation);
            }
            LootFunction::SetWrittenBookPages { pages, mode } => {
                item.set_written_book_pages(pages, *mode);
            }
            LootFunction::SetWritableBookPages { pages, mode } => {
                item.set_writable_book_pages(pages, *mode);
            }
            LootFunction::ToggleTooltips { toggles } => {
                item.toggle_tooltips(toggles);
            }
            LootFunction::Discard => {
                item.count = 0;
            }
            LootFunction::Reference(_name) => {
                // TODO: Implement function registry lookup
            }
            LootFunction::Sequence { functions } => {
                for cond_func in *functions {
                    if cond_func.conditions.iter().all(|c| c.test(ctx)) {
                        cond_func.function.apply(item, ctx);
                    }
                }
            }
            LootFunction::Filtered {
                item_filter,
                on_pass,
                on_fail,
            } => {
                let branch = if item_filter.test(item) {
                    on_pass
                } else {
                    on_fail
                };
                let Some(branch) = branch else { return };
                if branch.conditions.iter().all(|c| c.test(ctx)) {
                    branch.function.apply(item, ctx);
                }
            }
        }
    }
}

impl BonusFormula {
    /// Apply the bonus formula to calculate new count.
    pub fn apply<R: rand::Rng>(&self, count: i32, level: i32, rng: &mut R) -> i32 {
        match self {
            BonusFormula::OreDrops => {
                if level > 0 {
                    // Vanilla: count * (max(0, random(0..level+2) - 1) + 1)
                    let bonus = rng.random_range(0..level + 2) - 1;
                    let multiplier = bonus.max(0) + 1;
                    count * multiplier
                } else {
                    count
                }
            }
            BonusFormula::UniformBonusCount { bonus_multiplier } => {
                // Vanilla: count + random.nextInt(bonusMultiplier * level + 1),
                // with no level-zero shortcut. The value is the same either way
                // at level 0, but keeping the draw matches vanilla's shape for
                // when loot rolls move onto a seeded vanilla RNG.
                count + rng.random_range(0..bonus_multiplier * level + 1)
            }
            BonusFormula::BinomialWithBonusCount { extra, probability } => {
                // Vanilla: for each of (level + extra) trials, probability p to add 1
                let trials = level + extra;
                let mut bonus = 0;
                for _ in 0..trials {
                    if rng.random::<f32>() < *probability {
                        bonus += 1;
                    }
                }
                count + bonus
            }
        }
    }
}
