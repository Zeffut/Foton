//! The two items every mob answers the same way.
//!
//! Vanilla runs `Mob.checkAndHandleImportantInteractions` *before* the mob's
//! own `mobInteract`, and the order is the whole feature: a saddled pig's
//! right click means "get on", a villager's means "trade", and a name tag held
//! out to either would never be read if it came second.

use super::*;
use crate::behavior::{InteractionResult, init_behaviors};
use crate::entity::entities::CowEntity;
use crate::entity::next_entity_id;
use crate::entity::registry::init_entities;
use crate::player::{Player, ResetReason};
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use steel_registry::data_components::vanilla_components::CUSTOM_NAME;

/// The spot in the test chunk everything stands on.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn interaction_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    // A spawn egg builds its offspring through the entity factory registry, so
    // this needs more than the block/item behaviors.
    init_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

/// Builds a player who is really in `world` -- riding needs to find them there.
fn player_holding(world: &Arc<World>, stack: ItemStack) -> Arc<Player> {
    let player = TestPlayerBuilder::new(Arc::clone(world), "Namer", next_entity_id()).build();
    assert!(
        world.add_player(Arc::clone(&player), ResetReason::InitialJoin),
        "the test player should join the world"
    );
    player.inventory.lock().set_selected_item(stack);
    player
}

fn name_tag(name: &'static str) -> ItemStack {
    let mut stack = ItemStack::new(&vanilla_items::NAME_TAG);
    stack.set(CUSTOM_NAME, TextComponent::plain(name));
    stack
}

fn saddled_pig(world: &Arc<World>) -> Arc<PigEntity> {
    let pig = Arc::new(PigEntity::new(
        &vanilla_entities::PIG,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&pig) as SharedEntity)
        .unwrap_or_else(|error| panic!("the pig should join the test world: {error:?}"));
    LivingEntity::set_item_slot(
        pig.as_ref(),
        EquipmentSlot::Saddle,
        ItemStack::new(&vanilla_items::SADDLE),
    );
    assert!(pig.is_saddled(), "the test pig needs its saddle on");
    pig
}

fn cow(world: &Arc<World>) -> Arc<CowEntity> {
    let cow = Arc::new(CowEntity::new(
        &vanilla_entities::COW,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&cow) as SharedEntity)
        .unwrap_or_else(|error| panic!("the cow should join the test world: {error:?}"));
    cow
}

/// Counts the entities of `entity_type` around `SPAWN`, ignoring `except`.
fn nearby_count(world: &Arc<World>, entity_type: EntityTypeRef, except: i32) -> usize {
    world
        .get_entities_in_aabb_matching(
            &WorldAabb::new(4.5, 60.0, 4.5, 12.5, 68.0, 12.5),
            |entity| entity.entity_type() == entity_type && entity.id() != except,
        )
        .len()
}

/// A saddled pig is the case the ordering exists for: its `mob_interact`
/// consumes every right click that is not food, so a name tag reaching it
/// second would put the player in the saddle and never name anything.
#[test]
fn a_name_tag_names_a_saddled_pig_instead_of_mounting_it() {
    let world = interaction_world("name_tag_beats_the_saddle");
    let pig = saddled_pig(&world);
    let player = player_holding(&world, name_tag("Hamlet"));

    let result = Mob::interact_mob(
        pig.as_ref(),
        player.as_ref(),
        InteractionHand::MainHand,
        DVec3::ZERO,
    );

    assert_eq!(result, InteractionResult::Success);
    assert_eq!(
        pig.custom_name().map(|name| name.content),
        Some(TextComponent::plain("Hamlet").content),
        "the name tag should have named the pig"
    );
    assert!(
        !pig.is_vehicle(),
        "naming a pig must not also put the player on it"
    );
    assert!(
        player.inventory.lock().get_selected_item().is_empty(),
        "the name tag is spent"
    );
    assert!(
        pig.is_persistence_required(),
        "a named mob stops despawning"
    );
}

/// A blank name tag is not an important interaction, so the saddled pig gets
/// its own right click back. Without this the guard could be "name tag in
/// hand" rather than "name tag with a name on it" and nothing would notice.
#[test]
fn a_blank_name_tag_still_lets_a_saddled_pig_be_ridden() {
    let world = interaction_world("blank_name_tag_falls_through");
    let pig = saddled_pig(&world);
    let player = player_holding(&world, ItemStack::new(&vanilla_items::NAME_TAG));

    let result = Mob::interact_mob(
        pig.as_ref(),
        player.as_ref(),
        InteractionHand::MainHand,
        DVec3::ZERO,
    );

    assert_eq!(result, InteractionResult::Success);
    assert!(pig.custom_name().is_none(), "there was no name to apply");
    assert!(pig.is_vehicle(), "the ride should have happened instead");
}

/// A mob's own spawn egg breeds a baby out of it. This is the only way to get
/// a baby of something that cannot be bred with food.
#[test]
fn a_cow_spawn_egg_used_on_a_cow_leaves_a_calf() {
    let world = interaction_world("spawn_egg_breeds_a_calf");
    let cow = cow(&world);
    let player = player_holding(&world, ItemStack::new(&vanilla_items::COW_SPAWN_EGG));

    let result = Mob::interact_mob(
        cow.as_ref(),
        player.as_ref(),
        InteractionHand::MainHand,
        DVec3::ZERO,
    );

    assert_eq!(result, InteractionResult::SuccessServer);
    let calves = world.get_entities_in_aabb_matching(
        &WorldAabb::new(4.5, 60.0, 4.5, 12.5, 68.0, 12.5),
        |entity| entity.entity_type() == &vanilla_entities::COW && entity.id() != cow.id(),
    );
    assert_eq!(calves.len(), 1, "the egg should have left one calf");
    assert!(
        calves[0].as_mob().is_some_and(<dyn Mob>::is_baby),
        "a spawn egg used on a mob makes a baby, not another adult"
    );
    assert!(
        player.inventory.lock().get_selected_item().is_empty(),
        "the egg is spent"
    );
}

/// The egg has to be the mob's own. A chicken egg on a cow is not an important
/// interaction at all, so the click falls through to the cow's own handling --
/// which for a chicken egg is nothing.
#[test]
fn a_chicken_spawn_egg_on_a_cow_makes_nothing() {
    let world = interaction_world("wrong_spawn_egg_does_nothing");
    let cow = cow(&world);
    let player = player_holding(&world, ItemStack::new(&vanilla_items::CHICKEN_SPAWN_EGG));

    let result = Mob::interact_mob(
        cow.as_ref(),
        player.as_ref(),
        InteractionHand::MainHand,
        DVec3::ZERO,
    );

    assert_eq!(result, InteractionResult::Pass);
    assert_eq!(
        nearby_count(&world, &vanilla_entities::COW, cow.id()),
        0,
        "no cow should have been bred"
    );
    assert_eq!(
        nearby_count(&world, &vanilla_entities::CHICKEN, cow.id()),
        0,
        "and no chicken either -- the egg is not placed on a mob"
    );
    assert_eq!(
        player.inventory.lock().get_selected_item().count,
        1,
        "the egg is not spent"
    );
}
