//! Lowers a criterion's `trigger` + `conditions` into a `TriggerInstance`.
//!
//! The dispatch is exhaustive over the trigger ids vanilla's own advancement
//! data uses. A new trigger id, or a condition key that is not read here,
//! panics the build.

use proc_macro2::TokenStream;
use quote::quote;
use serde_json::Value;

use super::json::ObjectReader;
use super::predicates::{
    condition_terms, identifier, optional_condition_terms, optional_damage, optional_damage_source,
    optional_distance, optional_i32, optional_identifier, optional_int_bounds,
    optional_item_predicate, optional_location, required_identifier,
};

/// Emits the `TriggerInstance` for one criterion.
///
/// # Panics
/// On an unmodeled trigger id or an unread condition key.
pub fn trigger_instance(path: &str, trigger: &str, conditions: Option<&Value>) -> TokenStream {
    let mut reader = ObjectReader::new(path, conditions);
    let player = optional_condition_terms(&mut reader, "player");

    let instance = match trigger {
        "minecraft:impossible" => {
            assert!(
                reader.is_empty(),
                "{path}: `minecraft:impossible` takes no conditions"
            );
            quote! { TriggerInstance::Impossible }
        }

        "minecraft:tick" => quote! { TriggerInstance::Tick { player: #player } },
        "minecraft:location" => quote! { TriggerInstance::Location { player: #player } },
        "minecraft:slept_in_bed" => quote! { TriggerInstance::SleptInBed { player: #player } },
        "minecraft:hero_of_the_village" => {
            quote! { TriggerInstance::HeroOfTheVillage { player: #player } }
        }
        "minecraft:avoid_vibration" => {
            quote! { TriggerInstance::AvoidVibration { player: #player } }
        }
        "minecraft:started_riding" => {
            quote! { TriggerInstance::StartedRiding { player: #player } }
        }

        "minecraft:enchanted_item" => {
            let item = optional_item_predicate(&mut reader, "item");
            let levels = optional_int_bounds(&mut reader, "levels");
            quote! { TriggerInstance::EnchantedItem { player: #player, item: #item, levels: #levels } }
        }
        "minecraft:cured_zombie_villager" => {
            let villager = optional_condition_terms(&mut reader, "villager");
            let zombie = optional_condition_terms(&mut reader, "zombie");
            quote! {
                TriggerInstance::CuredZombieVillager {
                    player: #player, villager: #villager, zombie: #zombie,
                }
            }
        }
        "minecraft:brewed_potion" => {
            let potion = optional_identifier(&mut reader, "potion");
            quote! { TriggerInstance::BrewedPotion { player: #player, potion: #potion } }
        }

        "minecraft:player_killed_entity"
        | "minecraft:entity_killed_player"
        | "minecraft:kill_mob_near_sculk_catalyst" => {
            let entity = optional_condition_terms(&mut reader, "entity");
            let killing_blow = optional_damage_source(&mut reader, "killing_blow");
            let fields = quote! {
                player: #player, entity: #entity, killing_blow: #killing_blow
            };
            match trigger {
                "minecraft:player_killed_entity" => {
                    quote! { TriggerInstance::PlayerKilledEntity { #fields } }
                }
                "minecraft:entity_killed_player" => {
                    quote! { TriggerInstance::EntityKilledPlayer { #fields } }
                }
                _ => quote! { TriggerInstance::KillMobNearSculkCatalyst { #fields } },
            }
        }
        "minecraft:killed_by_arrow" => {
            let victims = predicate_list(&mut reader, "victims");
            let unique_entity_types = optional_int_bounds(&mut reader, "unique_entity_types");
            let fired_from_weapon = optional_item_predicate(&mut reader, "fired_from_weapon");
            quote! {
                TriggerInstance::KilledByArrow {
                    player: #player,
                    victims: #victims,
                    unique_entity_types: #unique_entity_types,
                    fired_from_weapon: #fired_from_weapon,
                }
            }
        }
        "minecraft:channeled_lightning" => {
            let victims = predicate_list(&mut reader, "victims");
            quote! { TriggerInstance::ChanneledLightning { player: #player, victims: #victims } }
        }
        "minecraft:spear_mobs" => {
            let count = optional_i32(&mut reader, "count");
            quote! { TriggerInstance::SpearMobs { player: #player, count: #count } }
        }

        "minecraft:changed_dimension" => {
            let from = optional_identifier(&mut reader, "from");
            let to = optional_identifier(&mut reader, "to");
            quote! { TriggerInstance::ChangedDimension { player: #player, from: #from, to: #to } }
        }
        "minecraft:nether_travel"
        | "minecraft:fall_from_height"
        | "minecraft:ride_entity_in_lava" => {
            let start_position = optional_location(&mut reader, "start_position");
            let distance = optional_distance(&mut reader, "distance");
            let fields = quote! {
                player: #player, start_position: #start_position, distance: #distance
            };
            match trigger {
                "minecraft:nether_travel" => quote! { TriggerInstance::NetherTravel { #fields } },
                "minecraft:fall_from_height" => {
                    quote! { TriggerInstance::FallFromHeight { #fields } }
                }
                _ => quote! { TriggerInstance::RideEntityInLava { #fields } },
            }
        }
        "minecraft:fall_after_explosion" => {
            let start_position = optional_location(&mut reader, "start_position");
            let distance = optional_distance(&mut reader, "distance");
            let cause = optional_condition_terms(&mut reader, "cause");
            quote! {
                TriggerInstance::FallAfterExplosion {
                    player: #player,
                    start_position: #start_position,
                    distance: #distance,
                    cause: #cause,
                }
            }
        }
        "minecraft:levitation" => {
            let distance = optional_distance(&mut reader, "distance");
            let duration = optional_int_bounds(&mut reader, "duration");
            quote! {
                TriggerInstance::Levitation {
                    player: #player, distance: #distance, duration: #duration,
                }
            }
        }

        "minecraft:construct_beacon" => {
            let level = optional_int_bounds(&mut reader, "level");
            quote! { TriggerInstance::ConstructBeacon { player: #player, level: #level } }
        }
        "minecraft:consume_item" => {
            let item = optional_item_predicate(&mut reader, "item");
            quote! { TriggerInstance::ConsumeItem { player: #player, item: #item } }
        }
        "minecraft:effects_changed" => {
            let effects = effect_matches(&mut reader);
            let source = optional_condition_terms(&mut reader, "source");
            quote! {
                TriggerInstance::EffectsChanged {
                    player: #player, effects: #effects, source: #source,
                }
            }
        }
        "minecraft:enter_block" | "minecraft:slide_down_block" => {
            let block = optional_identifier(&mut reader, "block");
            let state = block_state(&mut reader);
            let fields = quote! { player: #player, block: #block, state: #state };
            if trigger == "minecraft:enter_block" {
                quote! { TriggerInstance::EnterBlock { #fields } }
            } else {
                quote! { TriggerInstance::SlideDownBlock { #fields } }
            }
        }
        "minecraft:filled_bucket" => {
            let item = optional_item_predicate(&mut reader, "item");
            quote! { TriggerInstance::FilledBucket { player: #player, item: #item } }
        }
        "minecraft:fishing_rod_hooked" => {
            let rod = optional_item_predicate(&mut reader, "rod");
            let entity = optional_condition_terms(&mut reader, "entity");
            let item = optional_item_predicate(&mut reader, "item");
            quote! {
                TriggerInstance::FishingRodHooked {
                    player: #player, rod: #rod, entity: #entity, item: #item,
                }
            }
        }
        "minecraft:inventory_changed" => {
            let slots = slots_predicate(&mut reader);
            let items = item_predicate_list(&mut reader, "items");
            quote! {
                TriggerInstance::InventoryChanged {
                    player: #player, slots: #slots, items: #items,
                }
            }
        }
        "minecraft:item_durability_changed" => {
            let item = optional_item_predicate(&mut reader, "item");
            let durability = optional_int_bounds(&mut reader, "durability");
            let delta = optional_int_bounds(&mut reader, "delta");
            quote! {
                TriggerInstance::ItemDurabilityChanged {
                    player: #player, item: #item, durability: #durability, delta: #delta,
                }
            }
        }
        "minecraft:item_used_on_block"
        | "minecraft:allay_drop_item_on_block"
        | "minecraft:placed_block" => {
            let location = optional_condition_terms(&mut reader, "location");
            let fields = quote! { player: #player, location: #location };
            match trigger {
                "minecraft:item_used_on_block" => {
                    quote! { TriggerInstance::ItemUsedOnBlock { #fields } }
                }
                "minecraft:allay_drop_item_on_block" => {
                    quote! { TriggerInstance::AllayDropItemOnBlock { #fields } }
                }
                _ => quote! { TriggerInstance::PlacedBlock { #fields } },
            }
        }
        "minecraft:player_generates_container_loot" => {
            let loot_table = required_identifier(&mut reader, "loot_table");
            quote! {
                TriggerInstance::PlayerGeneratesContainerLoot {
                    player: #player, loot_table: #loot_table,
                }
            }
        }
        "minecraft:player_hurt_entity" => {
            let damage = optional_damage(&mut reader, "damage");
            let entity = optional_condition_terms(&mut reader, "entity");
            quote! {
                TriggerInstance::PlayerHurtEntity {
                    player: #player, damage: #damage, entity: #entity,
                }
            }
        }
        "minecraft:entity_hurt_player" => {
            let damage = optional_damage(&mut reader, "damage");
            quote! { TriggerInstance::EntityHurtPlayer { player: #player, damage: #damage } }
        }
        "minecraft:player_interacted_with_entity" | "minecraft:player_sheared_equipment" => {
            let item = optional_item_predicate(&mut reader, "item");
            let entity = optional_condition_terms(&mut reader, "entity");
            let fields = quote! { player: #player, item: #item, entity: #entity };
            if trigger == "minecraft:player_interacted_with_entity" {
                quote! { TriggerInstance::PlayerInteractedWithEntity { #fields } }
            } else {
                quote! { TriggerInstance::PlayerShearedEquipment { #fields } }
            }
        }
        "minecraft:recipe_crafted" | "minecraft:crafter_recipe_crafted" => {
            let recipe_id = required_identifier(&mut reader, "recipe_id");
            let ingredients = item_predicate_list(&mut reader, "ingredients");
            let fields = quote! {
                player: #player, recipe_id: #recipe_id, ingredients: #ingredients
            };
            if trigger == "minecraft:recipe_crafted" {
                quote! { TriggerInstance::RecipeCrafted { #fields } }
            } else {
                quote! { TriggerInstance::CrafterRecipeCrafted { #fields } }
            }
        }
        "minecraft:recipe_unlocked" => {
            let recipe = required_identifier(&mut reader, "recipe");
            quote! { TriggerInstance::RecipeUnlocked { player: #player, recipe: #recipe } }
        }
        "minecraft:shot_crossbow" => {
            let item = optional_item_predicate(&mut reader, "item");
            quote! { TriggerInstance::ShotCrossbow { player: #player, item: #item } }
        }
        "minecraft:summoned_entity" | "minecraft:tame_animal" => {
            let entity = optional_condition_terms(&mut reader, "entity");
            let fields = quote! { player: #player, entity: #entity };
            if trigger == "minecraft:summoned_entity" {
                quote! { TriggerInstance::SummonedEntity { #fields } }
            } else {
                quote! { TriggerInstance::TameAnimal { #fields } }
            }
        }
        "minecraft:target_hit" => {
            let signal_strength = optional_int_bounds(&mut reader, "signal_strength");
            let projectile = optional_condition_terms(&mut reader, "projectile");
            quote! {
                TriggerInstance::TargetHit {
                    player: #player, signal_strength: #signal_strength, projectile: #projectile,
                }
            }
        }
        "minecraft:thrown_item_picked_up_by_entity"
        | "minecraft:thrown_item_picked_up_by_player" => {
            let item = optional_item_predicate(&mut reader, "item");
            let entity = optional_condition_terms(&mut reader, "entity");
            let fields = quote! { player: #player, item: #item, entity: #entity };
            if trigger == "minecraft:thrown_item_picked_up_by_entity" {
                quote! { TriggerInstance::ThrownItemPickedUpByEntity { #fields } }
            } else {
                quote! { TriggerInstance::ThrownItemPickedUpByPlayer { #fields } }
            }
        }
        "minecraft:used_totem" | "minecraft:using_item" => {
            let item = optional_item_predicate(&mut reader, "item");
            let fields = quote! { player: #player, item: #item };
            if trigger == "minecraft:used_totem" {
                quote! { TriggerInstance::UsedTotem { #fields } }
            } else {
                quote! { TriggerInstance::UsingItem { #fields } }
            }
        }
        "minecraft:villager_trade" => {
            let villager = optional_condition_terms(&mut reader, "villager");
            let item = optional_item_predicate(&mut reader, "item");
            quote! {
                TriggerInstance::VillagerTrade {
                    player: #player, villager: #villager, item: #item,
                }
            }
        }
        "minecraft:bee_nest_destroyed" => {
            let block = optional_identifier(&mut reader, "block");
            let item = optional_item_predicate(&mut reader, "item");
            let num_bees_inside = optional_int_bounds(&mut reader, "num_bees_inside");
            quote! {
                TriggerInstance::BeeNestDestroyed {
                    player: #player, block: #block, item: #item,
                    num_bees_inside: #num_bees_inside,
                }
            }
        }
        "minecraft:bred_animals" => {
            let parent = optional_condition_terms(&mut reader, "parent");
            let partner = optional_condition_terms(&mut reader, "partner");
            let child = optional_condition_terms(&mut reader, "child");
            quote! {
                TriggerInstance::BredAnimals {
                    player: #player, parent: #parent, partner: #partner, child: #child,
                }
            }
        }
        "minecraft:lightning_strike" => {
            let lightning = optional_condition_terms(&mut reader, "lightning");
            let bystander = optional_condition_terms(&mut reader, "bystander");
            quote! {
                TriggerInstance::LightningStrike {
                    player: #player, lightning: #lightning, bystander: #bystander,
                }
            }
        }

        other => panic!(
            "{path}: unmodeled criterion trigger `{other}`. Add it to \
             `crate::advancement::trigger::TriggerInstance` and to this dispatch."
        ),
    };

    reader.finish();
    instance
}

/// A `Vec<ContextAwarePredicate>` field, such as `victims`.
fn predicate_list(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    let Some(value) = reader.take(key) else {
        return quote! { &[] };
    };
    let Value::Array(list) = &value else {
        panic!("{path}: expected a list of predicates, found {value}");
    };
    let entries = list
        .iter()
        .enumerate()
        .map(|(index, terms)| condition_terms(&format!("{path}[{index}]"), terms));
    quote! { &[#(#entries),*] }
}

/// A `List<ItemPredicate>` field, such as `items` or `ingredients`.
fn item_predicate_list(reader: &mut ObjectReader, key: &str) -> TokenStream {
    let path = reader.child_path(key);
    let Some(value) = reader.take(key) else {
        return quote! { &[] };
    };
    let Value::Array(list) = &value else {
        panic!("{path}: expected a list of item predicates, found {value}");
    };
    let entries = list
        .iter()
        .enumerate()
        .map(|(index, item)| super::predicates::item_predicate(&format!("{path}[{index}]"), item));
    quote! { &[#(#entries),*] }
}

/// The `slots` block of `inventory_changed`.
fn slots_predicate(reader: &mut ObjectReader) -> TokenStream {
    let Some(mut slots) = reader.take_object("slots") else {
        return quote! { SlotsPredicate::ANY };
    };
    let occupied = optional_int_bounds(&mut slots, "occupied");
    let full = optional_int_bounds(&mut slots, "full");
    let empty = optional_int_bounds(&mut slots, "empty");
    slots.finish();
    quote! { SlotsPredicate { occupied: #occupied, full: #full, empty: #empty } }
}

/// The `effects` map of `effects_changed`.
fn effect_matches(reader: &mut ObjectReader) -> TokenStream {
    let Some(effects) = reader.take_object("effects") else {
        return quote! { &[] };
    };
    let (path, entries) = effects.drain();
    let matches = entries.into_iter().map(|(key, value)| {
        let inner = ObjectReader::new(format!("{path}.{key}"), Some(&value));
        assert!(
            inner.is_empty(),
            "{path}.{key}: only the bare presence of an effect is modeled"
        );
        inner.finish();
        let effect = identifier(&format!("{path}.{key}"), &key);
        quote! { MobEffectMatch { effect: #effect } }
    });
    quote! { &[#(#matches),*] }
}

/// The `state` block of `enter_block` / `slide_down_block`.
fn block_state(reader: &mut ObjectReader) -> TokenStream {
    let Some(state) = reader.take_object("state") else {
        return quote! { &[] };
    };
    let (path, entries) = state.drain();
    let matches = entries.into_iter().map(|(name, expected)| {
        let expected = match &expected {
            Value::String(text) => text.clone(),
            Value::Bool(flag) => flag.to_string(),
            Value::Number(number) => number.to_string(),
            other => panic!("{path}.{name}: only exact state values are modeled, found {other}"),
        };
        quote! { StatePropertyMatch { name: #name, value: #expected } }
    });
    quote! { &[#(#matches),*] }
}
