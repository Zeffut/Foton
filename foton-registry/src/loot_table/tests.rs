use crate::data_components::vanilla_components::INSTRUMENT;
use crate::vanilla_instrument_tags::InstrumentTag;
use crate::vanilla_items;
use crate::{init_vanilla_registry, vanilla_loot_tables};

use super::*;
use crate::biome::BiomeRef;
use crate::blocks::block_state_ext::BlockStateExt;
use crate::blocks::properties::{BlockStateProperties, DoubleBlockHalf};
use crate::vanilla_blocks;
use foton_utils::BlockStateId;
use rand::SeedableRng;

fn test_rng() -> rand::rngs::StdRng {
    rand::rngs::StdRng::seed_from_u64(12345)
}

fn init_test_registries() {
    init_vanilla_registry();
}

#[test]
fn test_oak_log_loot() {
    init_test_registries();
    let mut rng = test_rng();

    let mut ctx = LootContext::new(&mut rng);
    let items = vanilla_loot_tables::BLOCKS_OAK_LOG.get_random_items(&mut ctx);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].count, 1);
    assert_eq!(items[0].item.key, Identifier::vanilla_static("oak_log"));
}

#[test]
fn set_instrument_selects_from_the_configured_holder_set() {
    init_test_registries();
    let mut rng = test_rng();
    let mut ctx = LootContext::new(&mut rng);
    let mut goat_horn = ItemStack::new(&vanilla_items::GOAT_HORN);
    let function = LootFunction::SetInstrument {
        options: InstrumentOptions::Tag(InstrumentTag::REGULAR_GOAT_HORNS),
    };

    function.apply(&mut goat_horn, &mut ctx);

    let selected = goat_horn
        .get(INSTRUMENT)
        .and_then(|component| component.instrument().as_reference())
        .expect("set_instrument should select a registered instrument");
    assert!(
        REGISTRY
            .instruments
            .is_in_tag(selected, &InstrumentTag::REGULAR_GOAT_HORNS)
    );
}

#[test]
fn test_diamond_ore_loot_no_silk_touch() {
    // Without silk touch, diamond ore should drop diamond (not the ore block)
    init_test_registries();
    let mut rng = test_rng();

    let mut ctx = LootContext::new(&mut rng);
    let items = vanilla_loot_tables::BLOCKS_DIAMOND_ORE.get_random_items(&mut ctx);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].count, 1);
    // Without silk touch enchantment, diamond ore drops diamond
    assert_eq!(items[0].item.key, Identifier::vanilla_static("diamond"));
}

#[test]
fn test_grass_block_loot_no_silk_touch() {
    // Without silk touch, grass block should drop dirt
    init_test_registries();
    let mut rng = test_rng();

    let mut ctx = LootContext::new(&mut rng);
    let items = vanilla_loot_tables::BLOCKS_GRASS_BLOCK.get_random_items(&mut ctx);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].count, 1);
    // Without silk touch, grass block drops dirt
    assert_eq!(items[0].item.key, Identifier::vanilla_static("dirt"));
}

#[test]
fn test_stone_loot_no_silk_touch() {
    // Without silk touch, stone should drop cobblestone
    init_test_registries();
    let mut rng = test_rng();

    let mut ctx = LootContext::new(&mut rng);
    let items = vanilla_loot_tables::BLOCKS_STONE.get_random_items(&mut ctx);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].count, 1);
    // Without silk touch, stone drops cobblestone
    assert_eq!(items[0].item.key, Identifier::vanilla_static("cobblestone"));
}

#[test]
fn test_pig_loot_drops_raw_porkchop_when_not_on_fire() {
    init_test_registries();
    let mut rng = test_rng();
    let pig_key = Identifier::vanilla_static("pig");
    let pig = EntityRef {
        entity_type: Some(&pig_key),
        flags: EntityRefFlags::default(),
        equipment: None,
        custom_name: None,
        ..EntityRef::default()
    };

    let mut ctx = LootContext::new(&mut rng).with_this_entity(pig);
    let items = vanilla_loot_tables::ENTITIES_PIG.get_random_items(&mut ctx);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item.key, Identifier::vanilla_static("porkchop"));
    assert!((1..=3).contains(&items[0].count));
}

#[test]
fn shearing_sheep_table_parses_the_flat_type_specific_sheep_key() {
    init_test_registries();

    let table = &vanilla_loot_tables::SHEARING_SHEEP;
    let pool = table
        .pools
        .first()
        .expect("shearing table should have a pool");
    let LootEntry::Alternatives { children, .. } = pool
        .entries
        .first()
        .expect("shearing pool should start with alternatives")
    else {
        panic!("shearing pool should use an alternatives entry");
    };

    let mut checked = 0;
    for child in *children {
        let LootEntry::LootTableRef { conditions, .. } = child else {
            continue;
        };
        let Some(LootCondition::EntityProperties { predicate, .. }) = conditions.first() else {
            continue;
        };
        assert!(
            predicate.sheep_color.is_some(),
            "branch should match its wool color"
        );
        assert_eq!(
            predicate.sheep_sheared,
            Some(false),
            "sheared must come from the flat minecraft:type_specific/sheep key"
        );
        checked += 1;
    }
    assert_eq!(
        checked, 16,
        "all sixteen color branches should carry the sheared predicate"
    );
}

#[test]
fn sheared_predicate_rejects_non_sheep_entities() {
    init_test_registries();
    let mut rng = test_rng();
    let pig_key = Identifier::vanilla_static("pig");
    let pig = EntityRef {
        entity_type: Some(&pig_key),
        flags: EntityRefFlags::default(),
        equipment: None,
        custom_name: None,
        ..EntityRef::default()
    };
    let mut ctx = LootContext::new(&mut rng).with_this_entity(pig);

    let condition = LootCondition::EntityProperties {
        entity: LootContextEntity::This,
        predicate: EntityPredicate {
            sheep_sheared: Some(false),
            ..EntityPredicate::ANY
        },
    };
    assert!(
        !condition.test(&mut ctx),
        "a non-sheep entity must fail a sheared predicate, mirroring SheepPredicate.matches"
    );
}

#[test]
fn test_pig_loot_smelt_condition_uses_entity_fire_flag() {
    init_test_registries();
    let mut rng = test_rng();
    let pig_key = Identifier::vanilla_static("pig");
    let pig = EntityRef {
        entity_type: Some(&pig_key),
        flags: EntityRefFlags {
            is_on_fire: true,
            ..EntityRefFlags::default()
        },
        equipment: None,
        custom_name: None,
        ..EntityRef::default()
    };

    let mut ctx = LootContext::new(&mut rng).with_this_entity(pig);
    let items = vanilla_loot_tables::ENTITIES_PIG.get_random_items(&mut ctx);

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].item.key,
        Identifier::vanilla_static("cooked_porkchop")
    );
    assert!((1..=3).contains(&items[0].count));
}

#[test]
fn test_uniform_get_int_reaches_inclusive_max() {
    // Vanilla UniformGenerator.getInt uses Mth.nextInt(rand, min, max), which
    // samples the integer range inclusively; a uniform 1..3 count must yield 3.
    let provider = NumberProvider::Uniform { min: 1.0, max: 3.0 };
    let mut seen = [false; 4];
    for seed in 0u64..1000 {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let value = provider.get_int(&mut rng);
        seen[value as usize] = true;
    }
    assert!(
        seen[1] && seen[2] && seen[3],
        "uniform 1..=3 must produce 1, 2 and 3, saw {seen:?}"
    );
}

#[test]
fn test_explosion_decay_function() {
    // Test the explosion_decay function directly
    init_test_registries();

    // explosion_decay reduces count based on 1/radius probability per item
    let cond_func = ConditionalLootFunction {
        function: LootFunction::ExplosionDecay,
        conditions: &[],
    };

    let mut total_survived = 0;
    let initial_count = 10;

    for seed in 0u64..100 {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng).with_explosion(4.0);
        let mut item = ItemStack::with_count(&crate::vanilla_items::STONE, initial_count);
        cond_func.function.apply(&mut item, &mut ctx);
        total_survived += item.count;
    }

    // With 10 items each trial, 100 trials = 1000 items total
    // Each has 25% (1/4.0) chance to survive = ~250 expected
    // Allow for variance: 150-350 range
    assert!(
        total_survived > 150 && total_survived < 350,
        "Expected ~250 items with explosion decay (25% of 1000), got {total_survived}"
    );
}

#[test]
fn ominous_bottle_amplifier_function_clamps_to_persistent_range() {
    use crate::data_components::vanilla_components::OMINOUS_BOTTLE_AMPLIFIER;

    init_test_registries();
    for (provided, expected) in [(-3.0, 0), (2.0, 2), (9.0, 4)] {
        let mut rng = test_rng();
        let mut context = LootContext::new(&mut rng);
        let mut item = ItemStack::new(&crate::vanilla_items::OMINOUS_BOTTLE);
        LootFunction::SetOminousBottleAmplifier {
            amplifier: NumberProvider::Constant(provided),
        }
        .apply(&mut item, &mut context);

        assert_eq!(
            item.get(OMINOUS_BOTTLE_AMPLIFIER)
                .map(|amplifier| amplifier.value()),
            Some(expected)
        );
    }
}

/// A tool carrying `enchantment` at `level`, for the tool-sensitive conditions.
fn enchanted_tool(
    item: &'static crate::items::Item,
    enchantment: &Identifier,
    level: u32,
) -> ItemStack {
    let mut tool = ItemStack::new(item);
    tool.set_enchantments(&[(enchantment.clone(), level)], false);
    tool
}

/// A killer holding `weapon`, which is where the looting primitives look.
fn killer_holding<'a>(
    entity_type: &'a Identifier,
    equipment: &'a EntityEquipmentRef<'a>,
) -> EntityRef<'a> {
    EntityRef {
        entity_type: Some(entity_type),
        flags: EntityRefFlags::default(),
        equipment: Some(equipment),
        custom_name: None,
        ..EntityRef::default()
    }
}

#[test]
fn silk_touch_makes_diamond_ore_drop_the_ore_block() {
    init_test_registries();
    let tool = enchanted_tool(
        &vanilla_items::DIAMOND_PICKAXE,
        &crate::vanilla_enchantments::SILK_TOUCH.key,
        1,
    );
    let mut rng = test_rng();
    let mut ctx = LootContext::new(&mut rng).with_tool(&tool);

    let items = vanilla_loot_tables::BLOCKS_DIAMOND_ORE.get_random_items(&mut ctx);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item.key, Identifier::vanilla_static("diamond_ore"));
}

#[test]
fn a_toolless_break_cannot_satisfy_a_tool_predicate() {
    // Vanilla `MatchTool.test` is `tool != null && ...`, so a roll with no TOOL
    // parameter fails even an empty predicate. Returning true here would hand
    // silk-touch drops to explosions, pistons and water flow.
    init_test_registries();
    let mut rng = test_rng();
    let mut ctx = LootContext::new(&mut rng);

    assert!(!LootCondition::MatchTool(ToolPredicate::Any).test(&mut ctx));
    assert!(
        !LootCondition::MatchTool(ToolPredicate::Item(Identifier::vanilla_static("shears")))
            .test(&mut ctx)
    );
}

#[test]
fn fortune_multiplies_diamond_ore_drops() {
    // `ore_drops` is `count * (max(0, nextInt(level + 2) - 1) + 1)`, so Fortune 3
    // can quadruple a single diamond while an unenchanted pick never can.
    init_test_registries();
    let fortune = enchanted_tool(
        &vanilla_items::DIAMOND_PICKAXE,
        &crate::vanilla_enchantments::FORTUNE.key,
        3,
    );
    let plain = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);

    let mut best_with_fortune = 0;
    let mut best_without = 0;
    for seed in 0u64..200 {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng).with_tool(&fortune);
        for item in vanilla_loot_tables::BLOCKS_DIAMOND_ORE.get_random_items(&mut ctx) {
            best_with_fortune = best_with_fortune.max(item.count);
        }

        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng).with_tool(&plain);
        for item in vanilla_loot_tables::BLOCKS_DIAMOND_ORE.get_random_items(&mut ctx) {
            best_without = best_without.max(item.count);
        }
    }

    assert_eq!(
        best_without, 1,
        "an unenchanted pick drops exactly one diamond"
    );
    assert!(
        best_with_fortune > 1,
        "Fortune 3 must sometimes drop more than one diamond, best was {best_with_fortune}"
    );
}

#[test]
fn apply_bonus_leaves_the_count_alone_without_a_tool() {
    // Vanilla `ApplyBonusCount.run` guards its whole body on the TOOL parameter.
    // `binomial_with_bonus_count` would otherwise grow the stack from its extra
    // rounds even though nothing was holding a tool.
    init_test_registries();
    let mut rng = test_rng();
    let mut ctx = LootContext::new(&mut rng);
    let mut item = ItemStack::with_count(&vanilla_items::DIAMOND, 1);

    LootFunction::ApplyBonus {
        enchantment: crate::vanilla_enchantments::FORTUNE.key.clone(),
        formula: BonusFormula::BinomialWithBonusCount {
            extra: 8,
            probability: 1.0,
        },
    }
    .apply(&mut item, &mut ctx);

    assert_eq!(item.count, 1);
}

#[test]
fn looting_on_the_killers_weapon_increases_mob_drops() {
    // Vanilla `EnchantedCountIncreaseFunction` reads ATTACKING_ENTITY, not TOOL,
    // because an entity loot roll never carries a tool at all. Reading the tool
    // made every mob drop as if the killer had Looting 0.
    init_test_registries();
    let sword = enchanted_tool(
        &vanilla_items::DIAMOND_SWORD,
        &crate::vanilla_enchantments::LOOTING.key,
        3,
    );
    let equipment = EntityEquipmentRef {
        mainhand: Some(&sword),
        offhand: None,
        head: None,
        chest: None,
        legs: None,
        feet: None,
    };
    let player_key = Identifier::vanilla_static("player");
    let zombie_key = Identifier::vanilla_static("zombie");

    let mut best_with_looting = 0;
    let mut best_without = 0;
    for seed in 0u64..200 {
        let zombie = EntityRef {
            entity_type: Some(&zombie_key),
            flags: EntityRefFlags::default(),
            equipment: None,
            custom_name: None,
            ..EntityRef::default()
        };

        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng)
            .with_this_entity(zombie)
            .with_killer_entity(killer_holding(&player_key, &equipment));
        for item in vanilla_loot_tables::ENTITIES_ZOMBIE.get_random_items(&mut ctx) {
            if item.item.key == Identifier::vanilla_static("rotten_flesh") {
                best_with_looting = best_with_looting.max(item.count);
            }
        }

        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng).with_this_entity(zombie);
        for item in vanilla_loot_tables::ENTITIES_ZOMBIE.get_random_items(&mut ctx) {
            if item.item.key == Identifier::vanilla_static("rotten_flesh") {
                best_without = best_without.max(item.count);
            }
        }
    }

    assert_eq!(
        best_without, 2,
        "an unenchanted killer is capped by the table's own `set_count` of 0-2"
    );
    assert!(
        best_with_looting > 2,
        "Looting 3 must push rotten flesh past 2, best was {best_with_looting}"
    );
}

#[test]
fn enchanted_count_increase_limits_the_whole_stack_not_the_bonus() {
    // Vanilla grows the stack and *then* calls `limitSize(limit)`. Clamping the
    // bonus instead lets the total run past the limit by the original count.
    init_test_registries();
    let sword = enchanted_tool(
        &vanilla_items::DIAMOND_SWORD,
        &crate::vanilla_enchantments::LOOTING.key,
        3,
    );
    let equipment = EntityEquipmentRef {
        mainhand: Some(&sword),
        offhand: None,
        head: None,
        chest: None,
        legs: None,
        feet: None,
    };
    let player_key = Identifier::vanilla_static("player");

    let mut rng = test_rng();
    let mut ctx =
        LootContext::new(&mut rng).with_killer_entity(killer_holding(&player_key, &equipment));
    let mut item = ItemStack::with_count(&vanilla_items::ROTTEN_FLESH, 3);

    LootFunction::EnchantedCountIncrease {
        enchantment: crate::vanilla_enchantments::LOOTING.key.clone(),
        count: NumberProvider::Constant(2.0),
        limit: 4,
    }
    .apply(&mut item, &mut ctx);

    // 3 + round(2 * 3) = 9, limited to 4. Clamping the bonus would give 3 + 4 = 7.
    assert_eq!(item.count, 4);
}

#[test]
fn explosion_decay_thins_block_drops_only_when_a_radius_is_present() {
    // The two halves of vanilla's DESTROY vs DESTROY_WITH_DECAY split: the same
    // table keeps every drop without EXPLOSION_RADIUS and loses most of them
    // with it.
    init_test_registries();
    let fortune = enchanted_tool(
        &vanilla_items::DIAMOND_PICKAXE,
        &crate::vanilla_enchantments::FORTUNE.key,
        3,
    );

    let mut kept_without_radius = 0;
    let mut kept_with_radius = 0;
    for seed in 0u64..300 {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng).with_tool(&fortune);
        kept_without_radius += vanilla_loot_tables::BLOCKS_DIAMOND_ORE
            .get_random_items(&mut ctx)
            .iter()
            .map(|item| item.count)
            .sum::<i32>();

        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng)
            .with_tool(&fortune)
            .with_explosion(4.0);
        kept_with_radius += vanilla_loot_tables::BLOCKS_DIAMOND_ORE
            .get_random_items(&mut ctx)
            .iter()
            .map(|item| item.count)
            .sum::<i32>();
    }

    assert!(
        kept_without_radius > 300,
        "no radius must keep every diamond"
    );
    assert!(
        kept_with_radius * 2 < kept_without_radius,
        "radius 4 keeps about a quarter of the drops, got {kept_with_radius} vs {kept_without_radius}"
    );
}

#[test]
fn chest_pool_weights_favor_the_heavier_entries() {
    // `chests/simple_dungeon` pool 0 weights name tags at 20 and enchanted
    // golden apples at 2, so the weighted pick has to reflect that ten-to-one
    // ratio rather than treating entries uniformly.
    init_test_registries();
    let name_tag = Identifier::vanilla_static("name_tag");
    let enchanted_apple = Identifier::vanilla_static("enchanted_golden_apple");

    let mut name_tags = 0;
    let mut enchanted_apples = 0;
    for seed in 0u64..3000 {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng);
        for item in vanilla_loot_tables::CHESTS_SIMPLE_DUNGEON.get_random_items(&mut ctx) {
            if item.item.key == name_tag {
                name_tags += 1;
            } else if item.item.key == enchanted_apple {
                enchanted_apples += 1;
            }
        }
    }

    assert!(
        enchanted_apples > 0,
        "the rare entry must still be reachable"
    );
    assert!(
        name_tags > enchanted_apples * 3,
        "weight 20 must beat weight 2 by a wide margin, got {name_tags} vs {enchanted_apples}"
    );
}

#[test]
fn an_unmodeled_entity_predicate_key_never_matches() {
    // `entities/zombie` gates its red mushroom on riding a zombie horse, a
    // vehicle predicate the generator cannot lower. Treating the unlowerable
    // predicate as satisfied handed every zombie a red mushroom.
    init_test_registries();
    let zombie_key = Identifier::vanilla_static("zombie");
    let red_mushroom = Identifier::vanilla_static("red_mushroom");

    for seed in 0u64..200 {
        let zombie = EntityRef {
            entity_type: Some(&zombie_key),
            ..EntityRef::default()
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng).with_this_entity(zombie);

        for item in vanilla_loot_tables::ENTITIES_ZOMBIE.get_random_items(&mut ctx) {
            assert_ne!(
                item.item.key, red_mushroom,
                "a zombie on foot must not drop the zombie-horse mushroom"
            );
        }
    }
}

#[test]
fn only_the_smallest_slime_drops_slime_balls() {
    // The whole pool is gated on `type_specific/cube_mob.size == 1`, so a big
    // slime yields nothing and only its smallest children pay out.
    init_test_registries();
    let slime_key = Identifier::vanilla_static("slime");

    let mut small_drops = 0;
    let mut large_drops = 0;
    for seed in 0u64..200 {
        for (size, counter) in [(1, &mut small_drops), (4, &mut large_drops)] {
            let slime = EntityRef {
                entity_type: Some(&slime_key),
                cube_size: Some(size),
                ..EntityRef::default()
            };
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let mut ctx = LootContext::new(&mut rng).with_this_entity(slime);
            *counter += vanilla_loot_tables::ENTITIES_SLIME
                .get_random_items(&mut ctx)
                .len();
        }
    }

    assert!(small_drops > 0, "a size-1 slime must drop slime balls");
    assert_eq!(large_drops, 0, "a size-4 slime must drop nothing itself");
}

#[test]
fn test_survives_explosion_condition() {
    init_test_registries();

    // Test that survives_explosion condition works
    // Gravel has survives_explosion on its alternatives
    let mut survived = 0;
    for seed in 0..100 {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng).with_explosion(4.0);
        let items = vanilla_loot_tables::BLOCKS_GRAVEL.get_random_items(&mut ctx);
        if !items.is_empty() {
            survived += 1;
        }
    }

    // With radius 4.0, ~25% should survive
    assert!(
        survived > 10 && survived < 50,
        "Expected ~25% survival rate, got {survived}%"
    );
}

/// Both of these functions were `const fn` bodies with a TODO in them, so the
/// tables that use them rolled an item they never touched. A junk fishing rod
/// came out of the water at full durability and the junk water bottle came out
/// as a bare `potion` item with nothing in it.
mod functions_that_used_to_do_nothing {
    use foton_utils::Identifier;

    use crate::init_vanilla_registry;
    use crate::item_stack::ItemStack;
    use crate::vanilla_items;

    /// Vanilla parity: `SetItemDamageFunction.run`. The fraction is of the
    /// whole bar, so 0.25 off a 64-use rod leaves 48 uses.
    #[test]
    fn setting_a_damage_fraction_takes_that_share_of_the_bar() {
        init_vanilla_registry();
        let mut rod = ItemStack::new(&vanilla_items::FISHING_ROD);
        let max = rod.get_max_damage();
        assert!(max > 0, "a fishing rod should be damageable");

        rod.set_damage_fraction(0.25, false);

        let expected = ((1.0 - 0.25) * max as f32).floor() as i32;
        assert_eq!(rod.get_damage_value(), expected);
        assert_ne!(rod.get_damage_value(), 0, "the rod came out untouched");
    }

    /// Vanilla parity: the `add` arm, which stacks onto the damage already
    /// there rather than replacing it.
    #[test]
    fn adding_a_damage_fraction_stacks_on_what_is_already_gone() {
        init_vanilla_registry();
        let mut rod = ItemStack::new(&vanilla_items::FISHING_ROD);
        rod.set_damage_fraction(0.25, false);
        let after_first = rod.get_damage_value();

        rod.set_damage_fraction(0.25, true);

        assert!(
            rod.get_damage_value() < after_first,
            "the second call did not take any more off"
        );
    }

    /// An item with no durability is left alone, which is what vanilla does
    /// after logging a warning.
    #[test]
    fn an_item_that_cannot_be_damaged_is_left_alone() {
        init_vanilla_registry();
        let mut stone = ItemStack::new(&vanilla_items::STONE);
        stone.set_damage_fraction(0.5, false);
        assert_eq!(stone.get_damage_value(), 0);
    }

    /// Vanilla parity: `SetPotionFunction.run`.
    #[test]
    fn setting_a_potion_puts_it_in_the_bottle() {
        init_vanilla_registry();
        let mut bottle = ItemStack::new(&vanilla_items::POTION);
        bottle.set_potion(&Identifier::vanilla_static("water"));

        let contents = bottle
            .get(crate::data_components::vanilla_components::POTION_CONTENTS)
            .expect("the bottle should carry potion contents now");
        assert!(
            contents.potion().is_some(),
            "the bottle came out with nothing in it"
        );
    }

    /// An id no potion answers to is ignored rather than written, so a bad
    /// table cannot produce a bottle of nothing. A fresh `potion` item already
    /// carries empty contents from its default components, so what this checks
    /// is the potion inside them, not the presence of the component.
    #[test]
    fn an_unknown_potion_id_leaves_the_bottle_as_it_was() {
        init_vanilla_registry();
        let mut bottle = ItemStack::new(&vanilla_items::POTION);
        let before = bottle
            .get(crate::data_components::vanilla_components::POTION_CONTENTS)
            .and_then(crate::data_components::components::PotionContents::potion)
            .map(|potion| potion.value().key.clone());

        bottle.set_potion(&Identifier::vanilla_static("not_a_potion"));

        let after = bottle
            .get(crate::data_components::vanilla_components::POTION_CONTENTS)
            .and_then(crate::data_components::components::PotionContents::potion)
            .map(|potion| potion.value().key.clone());
        assert_eq!(before, after, "an unknown id changed the bottle");
    }
}

/// A world that answers exactly what a test put in it.
///
/// Everything else reads as unloaded, which is what makes the "no world knows
/// about this position" branch of `LocationPredicate` testable.
struct TestWorld {
    biome: Option<BiomeRef>,
    blocks: Vec<((i32, i32, i32), BlockStateId)>,
}

impl TestWorld {
    const fn in_biome(biome: BiomeRef) -> Self {
        Self {
            biome: Some(biome),
            blocks: Vec::new(),
        }
    }

    const fn holding(blocks: Vec<((i32, i32, i32), BlockStateId)>) -> Self {
        Self {
            biome: None,
            blocks,
        }
    }
}

impl LootWorldView for TestWorld {
    fn loaded_block_state(&self, x: i32, y: i32, z: i32) -> Option<BlockStateId> {
        self.blocks
            .iter()
            .find(|(pos, _)| *pos == (x, y, z))
            .map(|(_, state)| *state)
    }

    fn loaded_biome(&self, x: i32, y: i32, z: i32) -> Option<BiomeRef> {
        let _ = (x, y, z);
        self.biome
    }
}

fn biome(key: &'static str) -> BiomeRef {
    REGISTRY
        .biomes
        .by_key(&Identifier::vanilla_static(key))
        .expect("vanilla biome")
}

/// Rolls `gameplay/fishing/junk` a fixed number of times in `world` and reports
/// whether bamboo ever came out.
fn junk_yields_bamboo(world: &TestWorld) -> bool {
    let bamboo = Identifier::vanilla_static("bamboo");
    let mut rng = test_rng();
    (0..400).any(|_| {
        let mut ctx = LootContext::new(&mut rng)
            .with_world(world)
            .with_origin(8.0, 62.0, 8.0);
        vanilla_loot_tables::GAMEPLAY_FISHING_JUNK
            .get_random_items(&mut ctx)
            .iter()
            .any(|item| item.item.key == bamboo)
    })
}

#[test]
fn fishing_junk_yields_bamboo_only_in_the_jungle() {
    init_test_registries();

    assert!(
        junk_yields_bamboo(&TestWorld::in_biome(biome("bamboo_jungle"))),
        "bamboo is a jungle-only junk catch and 400 rolls should have found it"
    );
    assert!(
        !junk_yields_bamboo(&TestWorld::in_biome(biome("plains"))),
        "bamboo must not be fishable out of a plains pond"
    );
}

#[test]
fn fishing_junk_withholds_bamboo_from_a_world_it_cannot_read() {
    init_test_registries();

    // Vanilla's `LocationPredicate.matches` fails a position it cannot load
    // rather than assuming the biome matches.
    let unreadable = TestWorld {
        biome: None,
        blocks: Vec::new(),
    };
    assert!(!junk_yields_bamboo(&unreadable));
}

/// The two states of a large fern, which is the block whose loot table asks
/// about its other half.
fn large_fern_halves() -> (BlockStateId, BlockStateId) {
    let lower = vanilla_blocks::LARGE_FERN.default_state().set_value(
        &BlockStateProperties::DOUBLE_BLOCK_HALF,
        DoubleBlockHalf::Lower,
    );
    let upper = vanilla_blocks::LARGE_FERN.default_state().set_value(
        &BlockStateProperties::DOUBLE_BLOCK_HALF,
        DoubleBlockHalf::Upper,
    );
    (lower, upper)
}

fn shear_large_fern_lower_half(world: &TestWorld) -> Vec<ItemStack> {
    let (lower, _) = large_fern_halves();
    let shears = ItemStack::new(&vanilla_items::SHEARS);
    let mut rng = test_rng();
    let mut ctx = LootContext::new(&mut rng)
        .with_world(world)
        .with_origin(0.0, 64.0, 0.0)
        .with_block_state(lower)
        .with_tool(&shears);
    vanilla_loot_tables::BLOCKS_LARGE_FERN.get_random_items(&mut ctx)
}

#[test]
fn a_large_fern_half_drops_only_while_its_other_half_stands() {
    init_test_registries();
    let (lower, upper) = large_fern_halves();

    let whole = TestWorld::holding(vec![((0, 64, 0), lower), ((0, 65, 0), upper)]);
    let dropped = shear_large_fern_lower_half(&whole);
    assert_eq!(dropped.len(), 1, "shearing a whole fern drops fern");
    assert_eq!(dropped[0].item.key, Identifier::vanilla_static("fern"));
    assert_eq!(dropped[0].count, 2);

    // With the upper half already gone the pool's `location_check` fails, which
    // is what stops a double plant from paying out once per half.
    let beheaded = TestWorld::holding(vec![((0, 64, 0), lower)]);
    assert!(
        shear_large_fern_lower_half(&beheaded).is_empty(),
        "a lone lower half must drop nothing"
    );
}

/// Rolls `table` once against a subject and its killer and reports the items.
fn roll_entity_table(
    table: &LootTable,
    this: EntityRef<'_>,
    killer: Option<EntityRef<'_>>,
) -> Vec<ItemStack> {
    let mut rng = test_rng();
    let mut ctx = LootContext::new(&mut rng).with_this_entity(this);
    if let Some(killer) = killer {
        ctx = ctx
            .with_killer_entity(killer)
            .with_damage_source(DamageSourceInfo {
                damage_type: Some(&MOB_ATTACK),
                tags: &[],
                is_direct: true,
            });
    }
    table.get_random_items(&mut ctx)
}

fn drops_item(items: &[ItemStack], key: &'static str) -> bool {
    let key = Identifier::vanilla_static(key);
    items.iter().any(|item| item.item.key == key)
}

fn pillager(raider: Option<RaiderStatus>) -> EntityRef<'static> {
    EntityRef {
        entity_type: Some(&PILLAGER),
        raider,
        ..EntityRef::default()
    }
}

static MOB_ATTACK: Identifier = Identifier::vanilla_static("mob_attack");
static PILLAGER: Identifier = Identifier::vanilla_static("pillager");
static FROG: Identifier = Identifier::vanilla_static("frog");
static ZOMBIE: Identifier = Identifier::vanilla_static("zombie");
static PLAYER: Identifier = Identifier::vanilla_static("player");
static CHICKEN: Identifier = Identifier::vanilla_static("chicken");
static WARM: Identifier = Identifier::vanilla_static("warm");
static COLD: Identifier = Identifier::vanilla_static("cold");

#[test]
fn only_a_patrol_captain_outside_a_raid_drops_the_ominous_bottle() {
    init_test_registries();
    let table = &vanilla_loot_tables::ENTITIES_PILLAGER;

    let captain = RaiderStatus {
        has_raid: false,
        is_captain: true,
    };
    assert!(drops_item(
        &roll_entity_table(table, pillager(Some(captain)), None),
        "ominous_bottle"
    ));

    // A captain leading a raid wave is a different subject: vanilla's
    // predicate wants `has_raid` false as well.
    let raid_captain = RaiderStatus {
        has_raid: true,
        is_captain: true,
    };
    assert!(
        roll_entity_table(table, pillager(Some(raid_captain)), None).is_empty(),
        "a raid captain is not a patrol captain"
    );

    let rank_and_file = RaiderStatus {
        has_raid: false,
        is_captain: false,
    };
    assert!(roll_entity_table(table, pillager(Some(rank_and_file)), None).is_empty());

    // Anything that is not a raider at all fails the predicate outright.
    assert!(roll_entity_table(table, pillager(None), None).is_empty());
}

#[test]
fn a_magma_cube_leaves_the_froglight_of_the_frog_that_ate_it() {
    init_test_registries();
    let table = &vanilla_loot_tables::ENTITIES_MAGMA_CUBE;
    let cube = EntityRef {
        entity_type: Some(&Identifier::vanilla_static("magma_cube")),
        cube_size: Some(1),
        ..EntityRef::default()
    };
    let frog = |variant: &'static Identifier| EntityRef {
        entity_type: Some(&FROG),
        frog_variant: Some(variant),
        ..EntityRef::default()
    };

    assert!(drops_item(
        &roll_entity_table(table, cube, Some(frog(&WARM))),
        "pearlescent_froglight"
    ));
    assert!(drops_item(
        &roll_entity_table(table, cube, Some(frog(&COLD))),
        "verdant_froglight"
    ));

    let player = EntityRef {
        entity_type: Some(&PLAYER),
        ..EntityRef::default()
    };
    let by_player = roll_entity_table(table, cube, Some(player));
    assert!(
        !drops_item(&by_player, "pearlescent_froglight")
            && !drops_item(&by_player, "verdant_froglight"),
        "only a frog turns a magma cube into a froglight"
    );
}

#[test]
fn only_a_chicken_jockey_drops_the_lava_chicken_disc() {
    init_test_registries();
    let table = &vanilla_loot_tables::ENTITIES_ZOMBIE;
    let jockey = EntityRef {
        entity_type: Some(&ZOMBIE),
        flags: EntityRefFlags {
            is_baby: true,
            ..EntityRefFlags::default()
        },
        vehicle_type: Some(&CHICKEN),
        ..EntityRef::default()
    };
    let mut rng = test_rng();
    let mut ctx = LootContext::new(&mut rng)
        .with_this_entity(jockey)
        .with_killed_by_player(true);
    assert!(drops_item(
        &table.get_random_items(&mut ctx),
        "music_disc_lava_chicken"
    ));

    let on_foot = EntityRef {
        vehicle_type: None,
        ..jockey
    };
    let mut ctx = LootContext::new(&mut rng)
        .with_this_entity(on_foot)
        .with_killed_by_player(true);
    assert!(!drops_item(
        &table.get_random_items(&mut ctx),
        "music_disc_lava_chicken"
    ));
}

#[test]
fn a_shipwreck_treasure_map_comes_out_named() {
    init_test_registries();
    let mut rng = test_rng();
    let mut ctx = LootContext::new(&mut rng);
    let items = vanilla_loot_tables::CHESTS_SHIPWRECK_MAP.get_random_items(&mut ctx);

    let map = items
        .iter()
        .find(|item| item.item.key == Identifier::vanilla_static("map"))
        .expect("the first pool of chests/shipwreck_map always rolls a map");
    let name = map
        .get(crate::data_components::vanilla_components::ITEM_NAME)
        .expect("a map always carries an item name; set_name replaces it");
    assert_eq!(
        name,
        &foton_utils::translations::FILLED_MAP_BURIED_TREASURE
            .msg()
            .into()
    );
    // `set_name` targets `item_name`, not `custom_name`: an anvil rename shows
    // in italics and this one must not.
    assert!(
        map.get(crate::data_components::vanilla_components::CUSTOM_NAME)
            .is_none()
    );
}

#[test]
fn a_silk_touched_beehive_carries_its_honey_level() {
    use crate::blocks::properties::BlockStateProperties;
    use crate::data_components::vanilla_components::BLOCK_STATE;

    init_test_registries();
    let tool = enchanted_tool(
        &vanilla_items::DIAMOND_AXE,
        &crate::vanilla_enchantments::SILK_TOUCH.key,
        1,
    );
    let half_full = vanilla_blocks::BEEHIVE
        .default_state()
        .set_value(&BlockStateProperties::LEVEL_HONEY, 5);
    let mut rng = test_rng();
    let mut ctx = LootContext::new(&mut rng)
        .with_tool(&tool)
        .with_block_state(half_full);

    let items = vanilla_loot_tables::BLOCKS_BEEHIVE.get_random_items(&mut ctx);

    assert_eq!(items.len(), 1);
    assert!(items[0].is(&vanilla_items::BEEHIVE));
    assert_eq!(
        items[0]
            .get(BLOCK_STATE)
            .and_then(|state| state.get("honey_level")),
        Some("5"),
        "a hive broken half full has to come up half full"
    );

    // Without silk touch the table takes its other alternative, which has no
    // `copy_state`, so the hive keeps the empty level the item ships with.
    let bare = ItemStack::new(&vanilla_items::DIAMOND_AXE);
    let mut ctx = LootContext::new(&mut rng)
        .with_tool(&bare)
        .with_block_state(half_full);
    let items = vanilla_loot_tables::BLOCKS_BEEHIVE.get_random_items(&mut ctx);
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0]
            .get(BLOCK_STATE)
            .and_then(|state| state.get("honey_level")),
        Some("0"),
        "the beehive item ships an empty honey level and nothing raised it"
    );
}

#[test]
fn trial_chamber_equipment_comes_out_trimmed() {
    use crate::data_components::vanilla_components::TRIM;

    init_test_registries();
    let mut rng = test_rng();
    let copper = Identifier::vanilla_static("copper");
    let bolt = Identifier::vanilla_static("bolt");
    let flow = Identifier::vanilla_static("flow");

    let mut trimmed = 0;
    for _ in 0..64 {
        let mut ctx = LootContext::new(&mut rng);
        for item in vanilla_loot_tables::EQUIPMENT_TRIAL_CHAMBER.get_random_items(&mut ctx) {
            let Some(trim) = item.get(TRIM) else {
                continue;
            };
            assert_eq!(
                trim.material().as_reference().map(|material| &material.key),
                Some(&copper)
            );
            let pattern = trim
                .pattern()
                .as_reference()
                .map(|pattern| &pattern.key)
                .expect("a generated trim names its pattern");
            assert!(
                *pattern == bolt || *pattern == flow,
                "the trial chamber only trims in bolt and flow, not {pattern}"
            );
            trimmed += 1;
        }
    }

    assert!(
        trimmed > 0,
        "sixty-four outfits should have carried at least one trimmed piece"
    );
}
