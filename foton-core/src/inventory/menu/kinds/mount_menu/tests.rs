//! Tests for the mount screen.

use std::sync::{Arc, Weak};

use foton_protocol::packet_traits::{CompressionInfo, EncodedPacket};
use foton_registry::packets::play::C_MOUNT_SCREEN_OPEN;
use foton_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
use foton_utils::ChunkPos;
use foton_utils::locks::SyncMutex;
use glam::DVec3;
use text_components::TextComponent;

use super::*;
use crate::entity::entities::{DonkeyEntity, HorseEntity, LlamaEntity};
use crate::entity::{AbstractChestedHorse, AbstractHorse, LivingEntity, Llama};
use crate::inventory::click::Click;
use crate::player::connection::{NetworkConnection, PlayerConnection};
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

/// Everything the server sent, shared with the test that reads it back.
type Recorded = Arc<SyncMutex<Vec<(i32, Vec<u8>)>>>;

/// A connection that keeps everything the server sends it.
struct RecordingConnection {
    packets: Recorded,
}

/// The payload of the first packet with `id`, or `None` if none was sent.
fn payload_of(recorded: &Recorded, id: i32) -> Option<Vec<u8>> {
    recorded
        .lock()
        .iter()
        .find(|(sent, _)| *sent == id)
        .map(|(_, payload)| payload.clone())
}

impl NetworkConnection for RecordingConnection {
    fn compression(&self) -> Option<CompressionInfo> {
        None
    }

    fn send_encoded(&self, packet: EncodedPacket) {
        // Uncompressed framing: a var-int body length, a var-int packet id,
        // then the payload.
        let bytes: &[u8] = &packet.encoded_data;
        let (_length, rest) = read_varint(bytes);
        let (id, payload) = read_varint(rest);
        self.packets.lock().push((id, payload.to_vec()));
    }

    fn send_encoded_bundle(&self, packets: Vec<EncodedPacket>) {
        for packet in packets {
            self.send_encoded(packet);
        }
    }

    fn disconnect_with_reason(&self, _reason: TextComponent) {}

    fn tick(&self) {}

    fn latency(&self) -> i32 {
        0
    }

    fn close(&self) {}

    fn closed(&self) -> bool {
        false
    }
}

fn read_varint(bytes: &[u8]) -> (i32, &[u8]) {
    let mut value = 0_i32;
    for index in 0..5 {
        let byte = bytes[index];
        value |= i32::from(byte & 0x7F) << (7 * index);
        if byte & 0x80 == 0 {
            return (value, &bytes[index + 1..]);
        }
    }
    panic!("var-int longer than five bytes");
}

fn tamed_horse() -> Arc<HorseEntity> {
    init_vanilla_registry();
    let horse = Arc::new(HorseEntity::new(
        &vanilla_entities::HORSE,
        7,
        DVec3::ZERO,
        Weak::new(),
    ));
    horse.set_tamed(true);
    horse
}

fn chested_donkey() -> Arc<DonkeyEntity> {
    init_vanilla_registry();
    let donkey = Arc::new(DonkeyEntity::new(
        &vanilla_entities::DONKEY,
        8,
        DVec3::ZERO,
        Weak::new(),
    ));
    donkey.set_tamed(true);
    donkey.set_chest(true);
    donkey.create_horse_inventory();
    donkey
}

/// Builds the screen a mount would open, bound to `player_inventory`.
fn menu_for(
    player_inventory: Shared<PlayerInventory>,
    mount: SharedEntity,
    columns: usize,
    inventory: Shared<SimpleContainer>,
) -> Menu {
    let Some(living) = mount.as_living_entity() else {
        panic!("a mount is a living entity");
    };
    let (equipment, saddle_index) = living
        .living_base()
        .equipment_slot_container(EquipmentSlot::Saddle);
    let body_index = living
        .living_base()
        .equipment_slot_container(EquipmentSlot::Body)
        .1;
    let entity_id = Entity::id(mount.as_ref());

    mount_menu(MountMenuParts {
        player_inventory,
        container_id: 1,
        mount,
        entity_id,
        equipment: ContainerRef::from(equipment),
        saddle_index,
        body_index,
        inventory,
        inventory_columns: columns,
        current_inventory: horse_inventory,
    })
}

/// Builds the screen for a mount with a throwaway player inventory.
fn detached_menu(mount: SharedEntity, columns: usize, inventory: Shared<SimpleContainer>) -> Menu {
    menu_for(
        PlayerInventory::new().into_shared(),
        mount,
        columns,
        inventory,
    )
}

#[test]
fn the_cargo_grid_is_three_rows_of_the_mount_own_column_count() {
    // Vanilla parity: `AbstractMountInventoryMenu.getInventorySize`. A horse
    // shows only the saddle and armor slots, a chested donkey five columns.
    let horse = tamed_horse();
    let horse_menu = detached_menu(
        Arc::clone(&horse) as SharedEntity,
        horse.inventory_columns(),
        horse.abstract_horse_base().inventory(),
    );
    assert_eq!(horse_menu.behavior().slot_count(), 2 + 36);

    let donkey = chested_donkey();
    let donkey_menu = detached_menu(
        Arc::clone(&donkey) as SharedEntity,
        donkey.inventory_columns(),
        donkey.abstract_horse_base().inventory(),
    );
    assert_eq!(donkey_menu.behavior().slot_count(), 2 + 5 * 3 + 36);
}

fn chested_llama(strength: i32) -> Arc<LlamaEntity> {
    init_vanilla_registry();
    let llama = Arc::new(LlamaEntity::new(
        &vanilla_entities::LLAMA,
        9,
        DVec3::ZERO,
        Weak::new(),
    ));
    llama.set_tamed(true);
    llama.set_chest(true);
    llama.set_strength(strength);
    llama.create_horse_inventory();
    llama
}

#[test]
fn a_llama_grid_is_as_wide_as_its_strength() {
    // Vanilla parity: `Llama.getInventoryColumns`, which is the strength rather
    // than the flat five every other chested horse has.
    let llama = chested_llama(3);

    let menu = detached_menu(
        Arc::clone(&llama) as SharedEntity,
        llama.inventory_columns(),
        llama.abstract_horse_base().inventory(),
    );

    assert_eq!(menu.behavior().slot_count(), 2 + 3 * 3 + 36);
}

#[test]
fn the_saddle_slot_writes_through_to_the_mount_equipment() {
    // The whole screen rests on this: its first two slots are not storage of
    // their own, they are the mount's worn equipment seen as a container. If
    // the two handles ever stopped being the same allocation, the screen would
    // quietly edit a copy nothing reads.
    let horse = tamed_horse();
    let menu = detached_menu(
        Arc::clone(&horse) as SharedEntity,
        horse.inventory_columns(),
        horse.abstract_horse_base().inventory(),
    );

    {
        let mut guard = menu.behavior().lock_all_containers();
        menu.behavior().slots()[0].set_item(&mut guard, ItemStack::new(&vanilla_items::SADDLE));
    }

    assert!(
        LivingEntity::get_item_by_slot(horse.as_ref(), EquipmentSlot::Saddle)
            .is(&vanilla_items::SADDLE)
    );
}

#[test]
fn the_armor_slot_only_takes_what_that_mount_can_wear() {
    // Vanilla parity: `ArmorSlot.mayPlace`, which asks the owner rather than
    // the item alone.
    let horse = tamed_horse();
    let menu = detached_menu(
        Arc::clone(&horse) as SharedEntity,
        horse.inventory_columns(),
        horse.abstract_horse_base().inventory(),
    );
    let body = &menu.behavior().slots()[1];
    assert!(body.may_place(&ItemStack::new(&vanilla_items::IRON_HORSE_ARMOR)));
    assert!(!body.may_place(&ItemStack::new(&vanilla_items::WHITE_CARPET)));
    assert!(!body.may_place(&ItemStack::new(&vanilla_items::SADDLE)));
}

#[test]
fn shift_clicking_armor_reaches_the_body_slot_before_the_cargo_grid() {
    // Vanilla parity: the branch order of `AbstractMountInventoryMenu.quickMoveStack`,
    // which offers the body slot first. The llama is the mount where it shows:
    // it is the only one that wears armor and carries cargo at once, so its
    // carpet has somewhere else it could wrongly land.
    init_vanilla_registry();
    let world = fresh_test_world("mount_menu_quick_move");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Rider", 1).build();

    let llama = chested_llama(3);
    let mut menu = menu_for(
        player.inventory.clone(),
        Arc::clone(&llama) as SharedEntity,
        llama.inventory_columns(),
        llama.abstract_horse_base().inventory(),
    );

    let player_slot = 2 + 3 * 3;
    {
        let mut guard = menu.behavior().lock_all_containers();
        menu.behavior().slots()[player_slot]
            .set_item(&mut guard, ItemStack::new(&vanilla_items::WHITE_CARPET));
    }

    menu.clicked(Click::QuickMove { slot: player_slot }, &player);

    let guard = menu.behavior().lock_all_containers();
    assert!(
        menu.behavior().slots()[1]
            .get_item(&guard)
            .is(&vanilla_items::WHITE_CARPET),
        "the carpet should have gone to the body slot",
    );
    assert!(
        menu.behavior().slots()[player_slot]
            .get_item(&guard)
            .is_empty()
    );
}

#[test]
fn shift_clicking_cargo_reaches_the_grid_and_not_the_armor_slots() {
    init_vanilla_registry();
    let world = fresh_test_world("mount_menu_quick_move_cargo");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Rider", 1).build();

    let donkey = chested_donkey();
    let mut menu = menu_for(
        player.inventory.clone(),
        Arc::clone(&donkey) as SharedEntity,
        donkey.inventory_columns(),
        donkey.abstract_horse_base().inventory(),
    );

    let player_slot = 2 + 5 * 3;
    {
        let mut guard = menu.behavior().lock_all_containers();
        menu.behavior().slots()[player_slot]
            .set_item(&mut guard, ItemStack::with_count(&vanilla_items::WHEAT, 12));
    }

    menu.clicked(Click::QuickMove { slot: player_slot }, &player);

    let guard = menu.behavior().lock_all_containers();
    assert_eq!(menu.behavior().slots()[2].get_item(&guard).count(), 12);
}

#[test]
fn the_screen_closes_when_the_mount_swaps_its_inventory() {
    // Vanilla parity: `AbstractHorse.hasInventoryChanged`. Strapping a chest
    // onto a donkey replaces its inventory with a wider one, and a screen still
    // pointing at the old one has to go.
    init_vanilla_registry();
    let world = fresh_test_world("mount_menu_inventory_swap");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Rider", 1).build();

    let donkey = chested_donkey();
    let menu = menu_for(
        player.inventory.clone(),
        Arc::clone(&donkey) as SharedEntity,
        donkey.inventory_columns(),
        donkey.abstract_horse_base().inventory(),
    );

    assert!(menu.still_valid(&player));

    donkey.create_horse_inventory();

    assert!(!menu.still_valid(&player));
}

#[test]
fn opening_a_horse_screen_sends_the_mount_screen_packet() {
    // The whole point of the feature, and the piece the rest of these tests
    // would miss: a menu that is built but whose packet is never sent looks
    // exactly like a working one from inside the server.
    init_vanilla_registry();
    let world = fresh_test_world("mount_menu_packet");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    let recorded: Recorded = Arc::new(SyncMutex::new(Vec::new()));
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Rider", 1)
        .connection(Arc::new(PlayerConnection::Other(Box::new(
            RecordingConnection {
                packets: Arc::clone(&recorded),
            },
        ))))
        .build();

    let horse = Arc::new(HorseEntity::new(
        &vanilla_entities::HORSE,
        7,
        DVec3::ZERO,
        Arc::downgrade(&world),
    ));
    horse.set_tamed(true);
    world
        .try_add_entity(Arc::clone(&horse) as SharedEntity)
        .expect("the test chunk is loaded, so the horse should attach");

    horse.open_custom_inventory_screen(&player);

    let payload = payload_of(&recorded, C_MOUNT_SCREEN_OPEN)
        .expect("the mount screen packet should have been sent");
    let (container_id, rest) = read_varint(&payload);
    let (columns, rest) = read_varint(rest);
    assert!(container_id > 0, "the menu must have its own container id");
    assert_eq!(columns, 0, "a plain horse carries no cargo");
    assert_eq!(
        i32::from_be_bytes(rest[..4].try_into().expect("four bytes of entity id")),
        Entity::id(horse.as_ref()),
    );
}
