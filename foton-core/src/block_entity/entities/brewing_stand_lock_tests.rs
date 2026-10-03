use super::*;
use crate::chunk::status::ChunkStatus;
use foton_protocol::utils::ConnectionProtocol;
use foton_registry::{init_vanilla_registry, vanilla_blocks};
use foton_utils::translations;
use simdnbt::borrow::read_compound;
use std::io::Cursor;

fn stand() -> BrewingStandBlockEntity {
    init_vanilla_registry();
    BrewingStandBlockEntity::new(
        Weak::new(),
        BlockPos::new(0, 64, 0),
        vanilla_blocks::BREWING_STAND.default_state(),
    )
}

fn load(stand: &BrewingStandBlockEntity, nbt: &NbtCompound) {
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_compound(&mut Cursor::new(bytes.as_slice())).expect("borrowed test NBT");
    stand.load_additional(&borrowed);
}

#[test]
fn lock_nbt_round_trip_and_absent_load_clears_old_lock() {
    let stand = stand();
    let mut predicate = NbtCompound::new();
    predicate.insert("items", "minecraft:tripwire_hook");
    let mut input = NbtCompound::new();
    input.insert("lock", predicate.clone());
    load(&stand, &input);
    assert_eq!(stand.save_custom_only().compound("lock"), Some(&predicate));
    load(&stand, &NbtCompound::new());
    assert!(stand.save_custom_only().get("lock").is_none());
}

fn key_lock() -> LockCode {
    use foton_registry::{
        RegistryHolderSet,
        data_component_predicate::DataComponentMatchers,
        item_predicate::{IntBounds, ItemPredicate},
    };
    LockCode::new(ItemPredicate::new(
        Some(RegistryHolderSet::Direct(vec![
            &vanilla_items::TRIPWIRE_HOOK,
        ])),
        IntBounds::ANY,
        DataComponentMatchers::ANY,
    ))
}

#[test]
fn implicit_lock_round_trip_clears_stored_and_previous_values() {
    use foton_registry::data_components::vanilla_components::{CUSTOM_NAME, LOCK};
    let first = stand();
    let mut item = ItemStack::new(&vanilla_items::BREWING_STAND);
    item.set(LOCK, key_lock());
    item.set(CUSTOM_NAME, TextComponent::plain("Keys"));
    first.apply_components_from_item_stack(&item);
    assert_eq!(first.lock_code(), key_lock());
    assert!(
        first.base().components().get(LOCK).is_none(),
        "LOCK is owned implicitly"
    );
    assert_eq!(first.collect_components().get(LOCK), Some(key_lock()));
    let restored = stand();
    load(&restored, &first.save_custom_only());
    assert_eq!(restored.lock_code(), key_lock());
    assert_eq!(restored.custom_name(), Some(TextComponent::plain("Keys")));
    first.apply_components_from_item_stack(&ItemStack::new(&vanilla_items::BREWING_STAND));
    assert_eq!(first.lock_code(), LockCode::NO_LOCK);
    assert!(first.collect_components().get(LOCK).is_none());
    assert!(first.unlocks_with(&ItemStack::empty()));
    let mut malformed = NbtCompound::new();
    malformed.insert("lock", 42);
    restored.set_lock_code(key_lock());
    load(&restored, &malformed);
    assert_eq!(restored.lock_code(), LockCode::NO_LOCK);
}

#[test]
fn lock_save_replaces_duplicate_tags_and_unlock_removes_previous_tag() {
    let original = stand();
    let mut nbt = NbtCompound::new();
    nbt.insert("lock", "stale");
    nbt.insert("lock", "also stale");
    original.set_lock_code(key_lock());
    original.save_additional(&mut nbt);
    assert_eq!(
        nbt.iter().filter(|(key, _)| key.to_str() == "lock").count(),
        1
    );
    let restored = stand();
    load(&restored, &nbt);
    assert_eq!(restored.lock_code(), key_lock());
    original.set_lock_code(LockCode::NO_LOCK);
    original.save_additional(&mut nbt);
    assert!(nbt.get("lock").is_none());
}

#[test]
fn air_key_persists_as_a_lock_that_accepts_an_empty_hand() {
    let original = stand();
    let mut predicate = NbtCompound::new();
    predicate.insert("items", "minecraft:air");
    let mut tag = NbtCompound::new();
    tag.insert("lock", predicate);
    load(&original, &tag);
    let restored = stand();
    load(&restored, &original.save_custom_only());
    assert_ne!(restored.lock_code(), LockCode::NO_LOCK);
    assert!(restored.unlocks_with(&ItemStack::empty()));
    assert!(!restored.unlocks_with(&ItemStack::new(&vanilla_items::TRIPWIRE_HOOK)));
}

use crate::{
    behavior::{
        BlockBehavior, BlockHitResult, InteractionResult, InventoryAccess,
        blocks::BrewingStandBlock, init_behaviors,
    },
    block_entity::init_block_entities,
    entity::Entity as _,
    player::{Player, PlayerConnection},
    test_support::{
        RecordingConnection, TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk,
    },
};
use foton_protocol::{
    packet_traits::EncodedPacket,
    packets::game::{CSystemChat, SoundSource},
};
use foton_registry::{
    packets::play::{C_SOUND, C_SYSTEM_CHAT},
    sound_events,
};
use foton_utils::{
    ChunkPos, Downcast as _,
    types::{GameType, InteractionHand},
};
use foton_utils::{codec::VarInt, serial::ReadFrom as _};
use glam::DVec3;

fn use_stand(
    world: &Arc<World>,
    player: &Player,
    pos: BlockPos,
    hand: InteractionHand,
) -> InteractionResult {
    BrewingStandBlock::new(&vanilla_blocks::BREWING_STAND).use_without_item(
        vanilla_blocks::BREWING_STAND.default_state(),
        world,
        pos,
        player,
        &BlockHitResult {
            location: DVec3::new(0.5, 64.5, 0.5),
            direction: Direction::Up,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        },
        &mut InventoryAccess::new(Arc::clone(&player.inventory), hand),
    )
}

#[test]
fn real_block_use_checks_main_hand_spectator_and_emits_locked_notifications() {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world("brewing_lock_use");
    let pos = BlockPos::new(0, 64, 0);
    let holder = insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos,
        vanilla_blocks::BREWING_STAND.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    let entity = world.get_block_entity(pos).expect("stand entity");
    let stand = entity
        .downcast_ref::<BrewingStandBlockEntity>()
        .expect("brewing stand");
    let chunk = holder.try_chunk(ChunkStatus::Full).expect("full chunk");
    chunk.clear_dirty();
    stand.set_lock_code(key_lock());
    assert!(
        chunk.is_dirty(),
        "replacing the lock marks its owning chunk dirty"
    );
    let sent = Arc::new(SyncMutex::new(Vec::<EncodedPacket>::new()));
    let player = TestPlayerBuilder::new(Arc::clone(&world), "KeyTester", 1)
        .connection(Arc::new(PlayerConnection::Other(Box::new(
            RecordingConnection::new(Arc::clone(&sent)),
        ))))
        .build();
    player.base().set_position_local(DVec3::new(0.5, 64.0, 0.5));
    assert!(world.players.insert(Arc::clone(&player)));
    world
        .player_area_map
        .on_player_view_change(player.id(), &[ChunkPos::from_block_pos(pos)], &[]);

    // Offhand-only keys fail even when that is the interaction hand.
    player
        .inventory
        .lock()
        .set_offhand_item(ItemStack::new(&vanilla_items::TRIPWIRE_HOOK));
    for name in [None, Some(TextComponent::plain("Private potions"))] {
        stand.name.set_custom_name(name.clone());
        sent.lock().clear();
        assert_eq!(
            use_stand(&world, &player, pos, InteractionHand::OffHand),
            InteractionResult::Success
        );
        assert!(!player.has_container_open());
        assert_locked_notifications(&player, &sent, name);
    }
    player
        .inventory
        .lock()
        .set_item(0, ItemStack::new(&vanilla_items::STONE));
    assert_eq!(
        use_stand(&world, &player, pos, InteractionHand::MainHand),
        InteractionResult::Success
    );
    assert!(!player.has_container_open());
    player
        .inventory
        .lock()
        .set_item(0, ItemStack::with_count(&vanilla_items::TRIPWIRE_HOOK, 17));
    use_stand(&world, &player, pos, InteractionHand::MainHand);
    assert!(player.has_container_open());
    assert_menu_slot_mapping(&player, stand);
    player.close_container();
    player.inventory.lock().set_item(0, ItemStack::empty());
    player.restore_game_modes(GameType::Spectator, None);
    use_stand(&world, &player, pos, InteractionHand::MainHand);
    assert!(player.has_container_open());
    player.close_container();
    player.restore_game_modes(GameType::Survival, None);
    stand.set_lock_code(LockCode::NO_LOCK);
    use_stand(&world, &player, pos, InteractionHand::MainHand);
    assert!(player.has_container_open());
}

fn assert_menu_slot_mapping(player: &Player, stand: &BrewingStandBlockEntity) {
    let items = [
        ItemStack::new(&vanilla_items::GLASS_BOTTLE),
        ItemStack::new(&vanilla_items::POTION),
        ItemStack::new(&vanilla_items::SPLASH_POTION),
        ItemStack::with_count(&vanilla_items::NETHER_WART, 3),
        ItemStack::with_count(&vanilla_items::BLAZE_POWDER, 4),
    ];
    for (slot, item) in items.iter().enumerate() {
        assert!(player.set_open_container_item(slot, item.clone()));
    }
    assert_eq!(stand.container.lock().items.as_slice(), &items);
    for (slot, item) in items.iter().enumerate() {
        assert_eq!(player.open_container_item(slot).as_ref(), Some(item));
    }
    assert!(!player.set_open_container_item(5, ItemStack::empty()));
}

fn assert_locked_notifications(
    player: &Player,
    sent: &SyncMutex<Vec<EncodedPacket>>,
    name: Option<TextComponent>,
) {
    let title =
        name.unwrap_or_else(|| TextComponent::translated(translations::CONTAINER_BREWING.msg()));
    let message = translations::CONTAINER_IS_LOCKED
        .message([title])
        .component();
    let expected = EncodedPacket::from_bare(
        CSystemChat::new(&message, true, player),
        None,
        ConnectionProtocol::Play,
    )
    .expect("overlay packet");
    let packets = sent.lock();
    let mut saw_chat = false;
    let mut saw_sound = false;
    for packet in packets.iter() {
        let mut input = Cursor::new(packet.encoded_data.as_slice());
        VarInt::read(&mut input).expect("packet length");
        match VarInt::read(&mut input).expect("packet id").0 {
            C_SYSTEM_CHAT => {
                assert_eq!(
                    packet.encoded_data.as_slice(),
                    expected.encoded_data.as_slice()
                );
                saw_chat = true;
            }
            C_SOUND => {
                assert_eq!(
                    VarInt::read(&mut input).expect("sound").0,
                    sound_events::BLOCK_CHEST_LOCKED.packet_holder_id()
                );
                assert_eq!(
                    VarInt::read(&mut input).expect("source").0,
                    SoundSource::Blocks.as_varint()
                );
                assert_eq!(i32::read(&mut input).expect("x"), 4);
                assert_eq!(i32::read(&mut input).expect("y"), 516);
                assert_eq!(i32::read(&mut input).expect("z"), 4);
                assert_eq!(
                    f32::read(&mut input).expect("volume").to_bits(),
                    1.0_f32.to_bits()
                );
                assert_eq!(
                    f32::read(&mut input).expect("pitch").to_bits(),
                    1.0_f32.to_bits()
                );
                saw_sound = true;
            }
            _ => {}
        }
    }
    assert!(
        saw_chat && saw_sound,
        "denial sends overlay and world-routed sound to the player"
    );
}
