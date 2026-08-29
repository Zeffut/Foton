//! The two map items: the blank map that becomes one, and the filled map that
//! draws the world under its holder.
//!
//! Vanilla parity: `MapItem` and `EmptyMapItem`.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::data_components::components::{MapDecorations, MapId, MapPostProcessing};
use foton_registry::data_components::vanilla_components::{
    MAP_DECORATIONS, MAP_ID, MAP_POST_PROCESSING,
};
use foton_registry::equipment::{EquipmentSlot, EquipmentSlotType};
use foton_registry::item_stack::ItemStack;
use foton_registry::map_color::{Brightness, MapColor};
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{REGISTRY, sound_events, vanilla_blocks, vanilla_items};
use foton_utils::{BlockPos, BlockStateId, ChunkPos, Direction, SectionPos};
use uuid::Uuid;

use crate::behavior::item_utils::create_filled_result;
use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext, UseOnContext};
use crate::chunk::heightmap::HeightmapType;
use crate::entity::{Entity as _, LivingEntity};
use crate::fluid::fluid_state_to_block;
use crate::map::saved_data::{MAP_SIZE, MapPlayerSource, MapPlayerState};
use crate::map::storage::{MapStorage, SharedMapData};
use crate::map::{MapFrame, MapItemSavedData};
use crate::player::Player;
use crate::world::{LevelReader as _, World};

/// Vanilla parity: `MapItem.IMAGE_WIDTH` and `IMAGE_HEIGHT`, which are equal.
const IMAGE_SIZE: i32 = MAP_SIZE as i32;

/// The filled map.
#[item_behavior]
pub struct MapItem;

impl ItemBehavior for MapItem {
    /// Vanilla parity: `MapItem.inventoryTick`.
    ///
    /// Deviation: this also runs `onCraftedPostProcess`. Vanilla runs it from
    /// `Slot.onTake`, which hands the taken stack to the item; Foton's
    /// `Slot::on_take` only sees the stack by reference, so a map that leaves a
    /// cartography table still carrying its `minecraft:map_post_processing`
    /// marker resolves it on the first tick after it lands in a real slot. The
    /// marker is what the tooltip reads while the map sits in the result slot,
    /// so resolving it any earlier would be wrong.
    fn inventory_tick(
        &self,
        stack: &mut ItemStack,
        world: &Arc<World>,
        owner: &dyn LivingEntity,
        slot: Option<EquipmentSlot>,
    ) {
        let Some(player) = owner.as_player() else {
            return;
        };
        apply_post_processing(stack, world, player);
        let Some(map_id) = stack.get(MAP_ID).copied() else {
            return;
        };
        let Some(data) = saved_data(world, player, map_id) else {
            return;
        };

        let decorations = stack
            .get(MAP_DECORATIONS)
            .cloned()
            .unwrap_or(MapDecorations::EMPTY);
        tick_carried_by(world, player, map_id, &data, &decorations, None);

        let locked = data.lock().locked;
        if !locked && slot.is_some_and(|slot| slot.slot_type() == EquipmentSlotType::Hand) {
            update(world, player, &data);
        }
    }

    /// Vanilla parity: `MapItem.useOn`, which marks the banner it clicked.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let pos = context.hit_result.block_pos;
        if !context
            .world
            .get_block_state(pos)
            .get_block()
            .has_tag(&BlockTag::BANNERS)
        {
            return InteractionResult::Pass;
        }

        let map_id = context.inv.with_item(|stack| stack.get(MAP_ID).copied());
        let Some(map_id) = map_id else {
            return InteractionResult::Success;
        };
        let Some(data) = saved_data(context.world, context.player, map_id) else {
            return InteractionResult::Success;
        };
        if !data.lock().toggle_banner(context.world, pos) {
            return InteractionResult::Fail;
        }
        InteractionResult::Success
    }
}

/// The blank map.
#[item_behavior]
pub struct EmptyMapItem;

impl ItemBehavior for EmptyMapItem {
    /// Vanilla parity: `EmptyMapItem.use`.
    ///
    /// Deviation: vanilla hands the new map straight back through
    /// `InteractionResult.heldItemTransformedTo` when the last blank map was
    /// spent, and otherwise adds it to the inventory. Foton has no
    /// "transformed to" result, so both cases go through
    /// `ItemUtils.createFilledResult`, which puts the map in the emptied hand
    /// when it can and in the inventory or on the ground when it cannot.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let Some(storage) = map_storage(context.world, context.player) else {
            return InteractionResult::Fail;
        };

        context.world.play_sound_at(
            &sound_events::UI_CARTOGRAPHY_TABLE_TAKE_RESULT,
            SoundSource::Players,
            context.player.position(),
            1.0,
            1.0,
            None,
        );

        let position = context.player.position();
        let map = create_map(
            context.world,
            &storage,
            position.x.floor() as i32,
            position.z.floor() as i32,
            0,
            true,
            false,
        );
        create_filled_result(context, map, true);
        InteractionResult::Success
    }
}

/// Vanilla parity: `MapItem.create`.
pub fn create_map(
    world: &Arc<World>,
    storage: &Arc<MapStorage>,
    origin_x: i32,
    origin_z: i32,
    scale: u8,
    tracking_position: bool,
    unlimited_tracking: bool,
) -> ItemStack {
    let mut map = ItemStack::new(&vanilla_items::FILLED_MAP);
    let data = MapItemSavedData::create_fresh(
        f64::from(origin_x),
        f64::from(origin_z),
        scale,
        tracking_position,
        unlimited_tracking,
        world.key.clone(),
        world.is_nether(),
    );
    let id = storage.next_id();
    storage.set(id, data);
    map.set(MAP_ID, id);
    map
}

/// Vanilla parity: `MapItem.onCraftedPostProcess`, which is what turns the
/// cartography table's `MAP_POST_PROCESSING` marker into a real new map.
pub fn apply_post_processing(stack: &mut ItemStack, world: &Arc<World>, player: &Player) {
    let Some(post_processing) = stack.get(MAP_POST_PROCESSING).copied() else {
        return;
    };
    stack.remove(MAP_POST_PROCESSING);

    let Some(storage) = map_storage(world, player) else {
        return;
    };
    let Some(map_id) = stack.get(MAP_ID).copied() else {
        return;
    };
    let Some(original) = storage.get(map_id) else {
        return;
    };

    let replacement = match post_processing {
        MapPostProcessing::Lock => original.lock().locked_copy(),
        MapPostProcessing::Scale => original.lock().scaled(),
    };
    let id = storage.next_id();
    storage.set(id, replacement);
    stack.set(MAP_ID, id);
}

/// Vanilla parity: `MapItem.getSavedData`.
#[must_use]
pub fn saved_data(world: &Arc<World>, player: &Player, id: MapId) -> Option<SharedMapData> {
    map_storage(world, player)?.get(id)
}

fn map_storage(world: &Arc<World>, player: &Player) -> Option<Arc<MapStorage>> {
    player.server().map_data.for_world(world).map(Arc::clone)
}

/// Resolves the other players a map is tracking, anywhere on the server.
///
/// Vanilla holds live `Player` references in `carriedBy`, so a holder who walks
/// into another dimension is still resolvable. Foton looks them up by UUID
/// across every online player for the same reason: scoping the lookup to one
/// world would drop and re-add such a holder every tick, and each re-add would
/// resend the whole 128x128 image.
struct OnlineMapHolders<'a> {
    player: &'a Player,
    map_id: MapId,
}

impl MapPlayerSource for OnlineMapHolders<'_> {
    fn holder(&self, uuid: Uuid) -> Option<MapPlayerState> {
        let holder = self.player.server().get_player_by_uuid(uuid)?;
        Some(player_state(
            &holder,
            self.map_id,
            holds_map(&holder, self.map_id),
        ))
    }
}

/// Vanilla parity: `MapItemSavedData.mapMatcher` applied to the whole inventory.
fn holds_map(player: &Player, map_id: MapId) -> bool {
    let inventory = player.inventory.lock();
    inventory
        .get_items()
        .iter()
        .any(|stack| stack.is(&vanilla_items::FILLED_MAP) && stack.get(MAP_ID) == Some(&map_id))
}

fn player_state(player: &Player, _map_id: MapId, holds_map: bool) -> MapPlayerState {
    let position = player.position();
    let (yaw, _) = player.rotation();
    MapPlayerState {
        uuid: player.gameprofile.id,
        name: player.gameprofile.name.clone(),
        x: position.x,
        z: position.z,
        y_rot: yaw,
        dimension: player.get_world().key.clone(),
        holds_map,
        map_invisible: map_invisibility_equipped(player),
    }
}

/// Vanilla parity: `MapItemSavedData.hasMapInvisibilityItemEquipped`.
///
/// The `minecraft:map_invisibility_equipment` item tag is empty in vanilla data
/// for this version, so nothing can currently satisfy it; the check is kept so
/// the behavior appears the moment the tag gains a member.
fn map_invisibility_equipped(player: &Player) -> bool {
    use crate::inventory::equipment::EntityEquipment as _;
    use foton_registry::vanilla_item_tags::ItemTag;

    let inventory = player.inventory.lock();
    EquipmentSlot::ALL
        .into_iter()
        .filter(|slot| !matches!(slot, EquipmentSlot::MainHand | EquipmentSlot::OffHand))
        .any(|slot| {
            inventory
                .get_ref(slot)
                .item()
                .has_tag(&ItemTag::MAP_INVISIBILITY_EQUIPMENT)
        })
}

/// Runs one holder's `tickCarriedBy` against the shared map.
pub fn tick_carried_by(
    world: &Arc<World>,
    player: &Player,
    map_id: MapId,
    data: &SharedMapData,
    decorations: &MapDecorations,
    frame: Option<MapFrame>,
) {
    // The ticking holder always matches: vanilla's `mapMatcher` short-circuits
    // on stack identity, and Foton has lifted the stack out of the slot for the
    // duration of the tick, so an inventory scan would answer wrongly.
    let state = player_state(player, map_id, true);
    let source = OnlineMapHolders { player, map_id };
    data.lock()
        .tick_carried_by(&state, &source, frame, decorations, world.game_time());
}

/// Vanilla parity: `MapItem.update`, the pass that reads the world under the
/// map and writes one stripe of pixels.
///
/// Deviation: vanilla calls `level.getChunk`, which generates the chunk if it
/// has to. Foton skips columns whose chunk is not loaded rather than driving
/// worldgen from an item tick; those pixels keep their previous color and are
/// filled in once the chunk is there.
#[expect(
    clippy::too_many_lines,
    reason = "one vanilla method whose loops only make sense together"
)]
pub fn update(world: &Arc<World>, player: &Player, data: &SharedMapData) {
    let mut map = data.lock();
    if map.dimension != world.key {
        return;
    }

    let scale = 1i32 << map.scale;
    let center_x = map.center_x;
    let center_z = map.center_z;
    let position = player.position();
    let player_img_x = (position.x.floor() as i32 - center_x) / scale + 64;
    let player_img_y = (position.z.floor() as i32 - center_z) / scale + 64;
    let mut radius = IMAGE_SIZE / scale;
    let has_ceiling = world.dimension_type.has_ceiling;
    if has_ceiling {
        radius /= 2;
    }

    let step = {
        let holder = map.holding_player_mut(player.gameprofile.id, &player.gameprofile.name);
        holder.step = holder.step.wrapping_add(1);
        holder.step
    };

    let min_y = world.get_min_y();
    let mut found_consecutive_changes = false;

    for img_x in (player_img_x - radius + 1)..(player_img_x + radius) {
        if (img_x & 15) != (step & 15) && !found_consecutive_changes {
            continue;
        }
        found_consecutive_changes = false;
        let mut previous_average_area_height = 0.0f64;

        for img_y in (player_img_y - radius - 1)..(player_img_y + radius) {
            if img_x < 0 || img_y < -1 || img_x >= IMAGE_SIZE || img_y >= IMAGE_SIZE {
                continue;
            }

            let distance_sqr = (img_x - player_img_x).pow(2) + (img_y - player_img_y).pow(2);
            let dither_black = distance_sqr > (radius - 2) * (radius - 2);
            let area_min_x = (center_x / scale + img_x - 64) * scale;
            let area_min_z = (center_z / scale + img_y - 64) * scale;
            let chunk = ChunkPos::new(
                SectionPos::block_to_section_coord(area_min_x),
                SectionPos::block_to_section_coord(area_min_z),
            );
            if world.chunk_map.with_full_chunk(chunk, |_| ()).is_none() {
                continue;
            }

            let mut counts: Vec<(MapColor, u32)> = Vec::new();
            let mut water_depth = 0i32;
            let mut average_area_height = 0.0f64;

            if has_ceiling {
                // Vanilla's deterministic Nether fog pattern, with Java's
                // wrapping integer arithmetic.
                let mut noise = area_min_x.wrapping_add(area_min_z.wrapping_mul(231_871));
                noise = noise
                    .wrapping_mul(noise)
                    .wrapping_mul(31_287_121)
                    .wrapping_add(noise.wrapping_mul(11));
                if (noise >> 20) & 1 == 0 {
                    add_color(&mut counts, default_map_color(&vanilla_blocks::DIRT), 10);
                } else {
                    add_color(&mut counts, default_map_color(&vanilla_blocks::STONE), 100);
                }
                average_area_height = 100.0;
            } else {
                for delta_x in 0..scale {
                    for delta_z in 0..scale {
                        let x = area_min_x + delta_x;
                        let z = area_min_z + delta_z;
                        let Some(first_available) =
                            world.height_at(HeightmapType::WorldSurface, x, z)
                        else {
                            continue;
                        };

                        let mut column_y = first_available;
                        let state = if column_y <= min_y {
                            REGISTRY
                                .blocks
                                .get_default_state_id(&vanilla_blocks::BEDROCK)
                        } else {
                            let mut pos;
                            let mut state;
                            loop {
                                column_y -= 1;
                                pos = BlockPos::new(x, column_y, z);
                                state = world.get_block_state(pos);
                                if state.get_map_color() != MapColor::NONE || column_y <= min_y {
                                    break;
                                }
                            }

                            if column_y > min_y && !state.get_fluid_state().is_empty() {
                                let mut solid_y = column_y - 1;
                                loop {
                                    let below = world.get_block_state(BlockPos::new(x, solid_y, z));
                                    solid_y -= 1;
                                    water_depth += 1;
                                    if solid_y <= min_y || below.get_fluid_state().is_empty() {
                                        break;
                                    }
                                }
                                state = correct_state_for_fluid_block(state, pos);
                            }
                            state
                        };

                        map.check_banners(world, x, z);
                        average_area_height += f64::from(column_y) / f64::from(scale * scale);
                        add_color(&mut counts, state.get_map_color(), 1);
                    }
                }
            }

            water_depth /= scale * scale;
            let color = highest_count_first(&counts);
            let checker = f64::from((img_x + img_y) & 1);
            let brightness = if color == MapColor::WATER {
                let diff = f64::from(water_depth).mul_add(0.1, checker * 0.2);
                if diff < 0.5 {
                    Brightness::High
                } else if diff > 0.9 {
                    Brightness::Low
                } else {
                    Brightness::Normal
                }
            } else {
                let diff = (average_area_height - previous_average_area_height) * 4.0
                    / f64::from(scale + 4)
                    + (checker - 0.5) * 0.4;
                if diff > 0.6 {
                    Brightness::High
                } else if diff < -0.6 {
                    Brightness::Low
                } else {
                    Brightness::Normal
                }
            };

            previous_average_area_height = average_area_height;
            if img_y >= 0
                && distance_sqr < radius * radius
                && (!dither_black || (img_x + img_y) & 1 != 0)
            {
                found_consecutive_changes |=
                    map.update_color(img_x as usize, img_y as usize, color.packed_id(brightness));
            }
        }
    }
}

/// Vanilla parity: `MapItem.getCorrectStateForFluidBlock`.
fn correct_state_for_fluid_block(state: BlockStateId, pos: BlockPos) -> BlockStateId {
    let fluid_state = state.get_fluid_state();
    if fluid_state.is_empty() || state.is_face_sturdy_at(pos, Direction::Up) {
        return state;
    }
    fluid_state_to_block(fluid_state)
}

fn default_map_color(block: BlockRef) -> MapColor {
    REGISTRY.blocks.get_default_state_id(block).get_map_color()
}

/// Vanilla parity: `LinkedHashMultiset.add`, which keeps first-seen order.
fn add_color(counts: &mut Vec<(MapColor, u32)>, color: MapColor, amount: u32) {
    if let Some(entry) = counts.iter_mut().find(|(seen, _)| *seen == color) {
        entry.1 += amount;
        return;
    }
    counts.push((color, amount));
}

/// Vanilla parity: `Iterables.getFirst(Multisets.copyHighestCountFirst(..), NONE)`.
///
/// Guava's sort is stable, so a tie is broken by which color was counted first.
fn highest_count_first(counts: &[(MapColor, u32)]) -> MapColor {
    let mut best = MapColor::NONE;
    let mut best_count = 0;
    for (color, count) in counts {
        if *count > best_count {
            best = *color;
            best_count = *count;
        }
    }
    best
}
