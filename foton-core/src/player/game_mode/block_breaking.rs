//! Block breaking state machine for players.
//!
//! This module implements the logic from Java's `ServerPlayerGameMode` for handling
//! block breaking, including progress tracking and validation.

use foton_registry::stat::Stat;
use foton_registry::vanilla_stat_types;
use std::sync::Arc;

use foton_protocol::packets::game::CBlockUpdate;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::data_components::AdventureModePredicate;
use foton_registry::data_components::vanilla_components::CAN_BREAK;
use foton_registry::equipment::EquipmentSlot;
use foton_registry::vanilla_attributes;
use foton_registry::{
    REGISTRY, blocks::properties::Direction, item_stack::ItemStack, vanilla_blocks,
    vanilla_game_events, vanilla_mob_effects,
};
use foton_utils::{
    BlockPos, BlockStateId,
    nbt::compare_nbt_compounds,
    types::{GameType, InteractionHand, UpdateFlags},
};

use crate::behavior::{BLOCK_BEHAVIORS, BlockLootContext, ITEM_BEHAVIORS};
use crate::block_entity::SharedBlockEntity;
use crate::entity::{Entity, LivingEntity};
use crate::fluid::fluid_state_to_block;
use crate::player::Player;
use crate::player::food_data::food_constants;
use crate::world::{ConditionalBlockSetResult, World, game_event::GameEventContext};

/// How much each level of haste or conduit power adds.
///
/// Vanilla parity: the `(amplification + 1) * 0.2F` of
/// `Player.getDestroySpeed`.
const DIG_SPEED_PER_LEVEL: f32 = 0.2;

/// How much slower mining is with both feet off the ground.
///
/// Vanilla parity: the `speed /= 5.0F` of `Player.getDestroySpeed`.
const AIRBORNE_MINING_PENALTY: f32 = 5.0;

impl Player {
    /// Mirrors vanilla `Player.blockActionRestricted` for block breaking.
    pub(super) fn block_action_restricted(&self, world: &World, pos: BlockPos) -> bool {
        let game_mode = self.game_mode();
        if !matches!(game_mode, GameType::Adventure | GameType::Spectator) {
            return false;
        }
        if game_mode == GameType::Spectator {
            return true;
        }
        if self.abilities.lock().may_build {
            return false;
        }

        // TODO: Retain Vanilla's mutable per-component AdventureModePredicate
        // cache once Foton's item components support that identity. Until then,
        // snapshotting safely releases the inventory lock but reevaluates each use.
        let can_break = {
            let inventory = self.inventory.lock();
            let item = inventory.get_selected_item();
            if item.is_empty() {
                return true;
            }
            item.get(CAN_BREAK).cloned()
        };
        let Some(can_break) = can_break else {
            return true;
        };
        !Self::can_break_block_in_adventure_mode(&can_break, world, pos)
    }

    fn can_break_block_in_adventure_mode(
        predicate: &AdventureModePredicate,
        world: &World,
        pos: BlockPos,
    ) -> bool {
        let state = world.get_block_state(pos);
        // Vanilla's BlockInWorld overload intentionally does not test the
        // predicate's block-entity component matchers.
        predicate.predicates().iter().any(|predicate| {
            if !predicate.matches_state(state) {
                return false;
            }
            let Some(expected_nbt) = predicate.nbt() else {
                return true;
            };
            let Some(block_entity) = world.get_block_entity(pos) else {
                return false;
            };
            let actual_nbt = block_entity.save_with_full_metadata();
            compare_nbt_compounds(expected_nbt.tag(), &actual_nbt, true)
        })
    }
}

/// Manages the block breaking state for a player.
///
/// Based on Java's `ServerPlayerGameMode` fields and logic.
pub struct BlockBreakingManager {
    /// Whether the player is currently breaking a block.
    is_destroying_block: bool,
    /// The tick when destruction started.
    destroy_progress_start: u64,
    /// The position of the block being destroyed.
    destroy_pos: BlockPos,
    /// The current game tick counter.
    game_ticks: u64,
    /// Whether there's a delayed destroy pending (for slow mining).
    has_delayed_destroy: bool,
    /// Position of the delayed destroy.
    delayed_destroy_pos: BlockPos,
    /// The tick when delayed destroy started.
    delayed_tick_start: u64,
    /// The last sent destruction progress state (0-9, or -1 for none).
    last_sent_state: i32,
}

impl Default for BlockBreakingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockBreakingManager {
    /// Creates a new block breaking manager.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            is_destroying_block: false,
            destroy_progress_start: 0,
            destroy_pos: BlockPos::new(0, 0, 0),
            game_ticks: 0,
            has_delayed_destroy: false,
            delayed_destroy_pos: BlockPos::new(0, 0, 0),
            delayed_tick_start: 0,
            last_sent_state: -1,
        }
    }

    /// Ticks the block breaking manager.
    ///
    /// This handles delayed destruction and updates break progress.
    pub fn tick(&mut self, player: &Player, world: &Arc<World>) {
        self.game_ticks += 1;

        if self.has_delayed_destroy {
            let state = world.get_block_state(self.delayed_destroy_pos);
            if is_air(state) {
                self.has_delayed_destroy = false;
            } else {
                let progress = self.increment_destroy_progress(
                    player,
                    world,
                    state,
                    self.delayed_destroy_pos,
                    self.delayed_tick_start,
                );
                if progress >= 1.0 {
                    self.has_delayed_destroy = false;
                    self.destroy_block(player, world, self.delayed_destroy_pos);
                }
            }
        } else if self.is_destroying_block {
            let state = world.get_block_state(self.destroy_pos);
            if is_air(state) {
                // Block was broken by something else
                world.broadcast_block_destruction(player.id(), self.destroy_pos, -1);
                self.last_sent_state = -1;
                self.is_destroying_block = false;
            } else {
                self.increment_destroy_progress(
                    player,
                    world,
                    state,
                    self.destroy_pos,
                    self.destroy_progress_start,
                );
            }
        }
    }

    /// Calculates and updates destruction progress, broadcasting to clients.
    fn increment_destroy_progress(
        &mut self,
        player: &Player,
        world: &Arc<World>,
        block_state: BlockStateId,
        pos: BlockPos,
        destroy_start_tick: u64,
    ) -> f32 {
        let ticks_spent = self.game_ticks.saturating_sub(destroy_start_tick);
        let destroy_speed = get_destroy_progress(player, block_state);
        let progress = destroy_speed * (ticks_spent + 1) as f32;
        let state = (progress * 10.0) as i32;

        if state != self.last_sent_state {
            world.broadcast_block_destruction(player.id(), pos, state);
            self.last_sent_state = state;
        }

        progress
    }

    /// Handles a block break action from the client.
    ///
    /// Note: The caller (packet handler) is responsible for calling `ack_block_changes_up_to`
    /// after this method returns, matching vanilla behavior.
    pub fn handle_block_break_action(
        &mut self,
        player: &Player,
        world: &Arc<World>,
        pos: BlockPos,
        action: BlockBreakAction,
        _direction: Direction,
    ) {
        // Validate interaction range
        if !player.is_within_block_interaction_range(pos) {
            return;
        }

        // Validate Y coordinate
        if pos.y() >= world.max_build_height() {
            player.send_packet(CBlockUpdate {
                pos,
                block_state: world.get_block_state(pos),
            });
            return;
        }

        match action {
            BlockBreakAction::Start => {
                // Check may_interact permission
                if !world.may_interact(player, pos) {
                    player.send_packet(CBlockUpdate {
                        pos,
                        block_state: world.get_block_state(pos),
                    });
                    return;
                }

                // Creative mode: instant break
                if player.game_mode() == GameType::Creative {
                    self.destroy_and_ack(player, world, pos);
                    return;
                }

                if player.block_action_restricted(world, pos) {
                    player.send_packet(CBlockUpdate {
                        pos,
                        block_state: world.get_block_state(pos),
                    });
                    return;
                }

                self.destroy_progress_start = self.game_ticks;
                let block_state = world.get_block_state(pos);

                if !is_air(block_state) {
                    // TODO: Call EnchantmentHelper.onHitBlock before blockState.attack.
                    BLOCK_BEHAVIORS
                        .get_behavior(block_state.get_block())
                        .attack(block_state, world, pos, player);

                    let progress = get_destroy_progress(player, block_state);

                    if progress >= 1.0 {
                        // Insta-mine
                        self.destroy_and_ack(player, world, pos);
                    } else {
                        // Start breaking
                        if self.is_destroying_block {
                            // Send block update for the old position to cancel client prediction
                            player.send_packet(CBlockUpdate {
                                pos: self.destroy_pos,
                                block_state: world.get_block_state(self.destroy_pos),
                            });
                        }

                        self.is_destroying_block = true;
                        self.destroy_pos = pos;
                        let state = (progress * 10.0) as i32;
                        world.broadcast_block_destruction(player.id(), pos, state);
                        self.last_sent_state = state;
                    }
                }
            }

            BlockBreakAction::Stop => {
                if pos == self.destroy_pos {
                    let ticks_spent = self.game_ticks.saturating_sub(self.destroy_progress_start);
                    let block_state = world.get_block_state(pos);

                    if !is_air(block_state) {
                        let destroy_speed = get_destroy_progress(player, block_state);
                        let progress = destroy_speed * (ticks_spent + 1) as f32;

                        if progress >= 0.7 {
                            // Complete the break
                            self.is_destroying_block = false;
                            world.broadcast_block_destruction(player.id(), pos, -1);
                            self.destroy_and_ack(player, world, pos);
                            return;
                        }

                        if !self.has_delayed_destroy {
                            // Set up delayed destroy
                            self.is_destroying_block = false;
                            self.has_delayed_destroy = true;
                            self.delayed_destroy_pos = pos;
                            self.delayed_tick_start = self.destroy_progress_start;
                        }
                    }
                }
            }

            BlockBreakAction::Abort => {
                self.is_destroying_block = false;

                if self.destroy_pos != pos {
                    log::warn!(
                        "Mismatch in destroy block pos: {:?} vs {:?}",
                        self.destroy_pos,
                        pos
                    );
                    world.broadcast_block_destruction(player.id(), self.destroy_pos, -1);
                }

                world.broadcast_block_destruction(player.id(), pos, -1);
            }
        }
    }

    /// Destroys a block and sends appropriate response.
    fn destroy_and_ack(&mut self, player: &Player, world: &Arc<World>, pos: BlockPos) {
        if !self.destroy_block(player, world, pos) {
            // Send block update to resync client
            player.send_packet(CBlockUpdate {
                pos,
                block_state: world.get_block_state(pos),
            });
        }
    }

    /// Destroys a block at the given position.
    ///
    /// Returns true if the block was successfully destroyed.
    #[expect(
        clippy::unused_self,
        reason = "method belongs logically to BlockBreakingManager and will use self when additional state is added"
    )]
    fn destroy_block(&self, player: &Player, world: &Arc<World>, pos: BlockPos) -> bool {
        let state = world.get_block_state(pos);

        if !item_can_destroy_block(player, world, pos, state) {
            return false;
        }

        // Get block info
        let Some(_block) = REGISTRY.blocks.by_state_id(state) else {
            return false;
        };

        // TODO: Check for GameMasterBlock (command blocks, etc.)
        // TODO: Check blockActionRestricted

        let behavior = BLOCK_BEHAVIORS.get_behavior(state.get_block());
        // Vanilla parity: `ServerPlayerGameMode.destroyBlock` reads the block
        // entity before it removes the block, then hands that one to the loot
        // roll and to `playerDestroy`. Looking it up afterwards finds nothing:
        // removing the block detaches it.
        let block_entity = world.get_block_entity(pos);
        let adjusted_state = behavior.player_will_destroy(state, world, pos, player);
        world.game_event(
            &vanilla_game_events::BLOCK_DESTROY,
            pos,
            &GameEventContext::new(Some(player), Some(adjusted_state)),
        );
        let state_after_player_will_destroy = world.get_block_state(pos);

        // Vanilla parity: fluidState.createLegacyBlock() — breaking a waterlogged
        // block leaves water behind instead of air.
        let replacement = fluid_state_to_block(state.get_fluid_state());
        // Vanilla removes the live state after `playerWillDestroy`; tripwire uses
        // that callback to set DISARMED before the same block is removed.
        let removed_by_player_break = !state_after_player_will_destroy.is_air()
            && world.set_block_if_unchanged(
                pos,
                state_after_player_will_destroy,
                replacement,
                UpdateFlags::UPDATE_ALL,
            ) == ConditionalBlockSetResult::Changed;
        let changed_by_player_will_destroy = state_after_player_will_destroy != state;
        let changed = changed_by_player_will_destroy || removed_by_player_break;

        if removed_by_player_break {
            behavior.destroy(adjusted_state, world, pos);

            // Play block destruction particles and sound (skip for fire blocks like vanilla)
            // Exclude the breaking player as they see the effect client-side
            let block = REGISTRY.blocks.by_state_id(adjusted_state);
            let is_fire = block.is_some_and(|b| {
                b.key == vanilla_blocks::FIRE.key || b.key == vanilla_blocks::SOUL_FIRE.key
            });
            if !is_fire {
                world.destroy_block_effect(pos, u32::from(adjusted_state.0), Some(player.id()));
            }

            // Vanilla snapshots the tool before Item.mineBlock can damage or
            // consume it, then uses that snapshot for loot and post-break effects.
            let (has_correct_tool, destroyed_with) = {
                let inv = player.inventory.lock();
                let main_hand = inv.get_item_in_hand(InteractionHand::MainHand);
                (
                    main_hand.is_correct_tool_for_drops(adjusted_state)
                        || !requires_correct_tool(adjusted_state),
                    main_hand.copy_with_count(main_hand.count),
                )
            };

            // Vanilla parity: `ItemStack.mineBlock`, dispatched to the item so the
            // ones with their own durability rule get it -- shears pay for a
            // zero-hardness plant and never pay for fire. Runs before
            // `playerDestroy`, as vanilla does.
            //
            // The tool can die here, and vanilla's `hurtAndBreak` announces
            // that itself. Foton's `ItemStack` cannot reach the miner, so the
            // break is read off the slot: the pickaxe was there before the
            // durability hit and is gone after.
            let tool_broke = {
                let mut inv = player.inventory.lock();
                let item_behavior = ITEM_BEHAVIORS.get_behavior(inv.get_selected_item().item());
                // with_selected_item_mut so the slot is marked changed.
                inv.with_selected_item_mut(|main_hand| {
                    let had_a_tool = !main_hand.is_empty();
                    item_behavior.mine_block(main_hand, adjusted_state, player);
                    had_a_tool && main_hand.is_empty()
                })
            };
            if tool_broke {
                LivingEntity::on_equipped_item_broken(player, EquipmentSlot::MainHand);
            }

            player.cause_food_exhaustion(food_constants::EXHAUSTION_MINE);

            // Handle drops (skip for creative/spectator)
            let game_mode = player.game_mode();
            if game_mode != GameType::Spectator
                && game_mode != GameType::Creative
                && has_correct_tool
            {
                drop_block_loot(
                    player,
                    world,
                    pos,
                    adjusted_state,
                    &destroyed_with,
                    block_entity.as_ref(),
                );
                // Vanilla parity: the `awardStat(Stats.BLOCK_MINED.get(this))`
                // that opens `Block.playerDestroy`, which Foton reaches through
                // the same non-creative, correct-tool gate this branch is under.
                player.award_stat(Stat::new(
                    &vanilla_stat_types::MINED,
                    adjusted_state.get_block(),
                ));
                behavior.player_destroy(
                    world,
                    player,
                    pos,
                    adjusted_state,
                    block_entity.as_ref(),
                    &destroyed_with,
                );
            }
        }

        changed
    }
}

/// Block break action types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockBreakAction {
    /// Player started breaking a block.
    Start,
    /// Player stopped breaking a block (finished or released).
    Stop,
    /// Player aborted breaking a block.
    Abort,
}

/// Checks if a block state is air.
fn is_air(state: BlockStateId) -> bool {
    let Some(block) = REGISTRY.blocks.by_state_id(state) else {
        return true;
    };
    block.config.is_air
}

/// Checks if a block requires the correct tool for drops.
fn requires_correct_tool(state: BlockStateId) -> bool {
    let Some(block) = REGISTRY.blocks.by_state_id(state) else {
        return false;
    };
    block.config.requires_correct_tool_for_drops
}

/// Gets the destroy progress per tick for a block.
///
/// This is based on the vanilla formula:
/// `1.0 / (destroy_time * 30.0)` for survival with correct tool
/// `1.0 / (destroy_time * 100.0)` for survival with wrong tool
/// Instant break for creative mode.
fn get_destroy_progress(player: &Player, block_state: BlockStateId) -> f32 {
    let Some(block) = REGISTRY.blocks.by_state_id(block_state) else {
        return 0.0;
    };

    let destroy_time = block.config.destroy_time;

    // Instant break for creative
    if player.game_mode() == GameType::Creative {
        return 1.0;
    }

    // Unbreakable block
    if destroy_time < 0.0 {
        return 0.0;
    }

    // Instant break for destroy_time == 0
    if destroy_time == 0.0 {
        return 1.0;
    }

    // Get player's mining speed
    let mining_speed = {
        let inv = player.inventory.lock();
        let main_hand = inv.get_item_in_hand(InteractionHand::MainHand);
        main_hand.get_destroy_speed(block_state)
    };

    // Check if player has the correct tool
    let has_correct_tool = {
        let inv = player.inventory.lock();
        let main_hand = inv.get_item_in_hand(InteractionHand::MainHand);
        main_hand.is_correct_tool_for_drops(block_state)
    };

    let speed = apply_mining_speed_modifiers(player, mining_speed);

    // Calculate destroy progress per tick
    // Vanilla formula: speed / hardness / (hasCorrectTool ? 30 : 100)
    let divisor = if has_correct_tool || !block.config.requires_correct_tool_for_drops {
        30.0
    } else {
        100.0
    };

    speed / destroy_time / divisor
}

/// Scales a tool's raw speed by everything the digger's own state does to it.
///
/// Vanilla parity: the body of `Player.getDestroySpeed` after the tool has been
/// asked, in vanilla's order -- the `MINING_EFFICIENCY` term is added before
/// haste and mining fatigue scale the result, and `BLOCK_BREAK_SPEED` and
/// `SUBMERGED_MINING_SPEED` multiply after them.
#[must_use]
pub(crate) fn apply_mining_speed_modifiers(player: &Player, tool_speed: f32) -> f32 {
    let mut speed = tool_speed;

    // Vanilla parity: `if (speed > 1.0F) speed += MINING_EFFICIENCY`. That
    // attribute is where Efficiency lands, and the `> 1.0` guard is why the
    // enchantment does nothing for a block the tool is not the tool for.
    if speed > 1.0 {
        speed += player
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MINING_EFFICIENCY) as f32;
    }

    // Vanilla parity: `MobEffectUtil.hasDigSpeed` and
    // `getDigSpeedAmplification`, which take the better of haste and conduit
    // power rather than stacking them.
    if let Some(amplification) = dig_speed_amplification(player) {
        speed *= 1.0 + (amplification + 1) as f32 * DIG_SPEED_PER_LEVEL;
    }

    // Vanilla parity: the `switch` of `Player.getDestroySpeed`. The steps are
    // steeper than a multiplier would be, which is why an elder guardian's
    // curse is worth swimming away from rather than working through.
    if let Some(fatigue) = player.mob_effect(vanilla_mob_effects::MINING_FATIGUE) {
        speed *= match fatigue.amplifier() {
            0 => 0.3,
            1 => 0.09,
            2 => 0.0027,
            _ => 0.000_81,
        };
    }

    // Vanilla parity: `speed *= BLOCK_BREAK_SPEED`, the attribute a command or
    // a plugin scales digging with. Its base value is 1, so this is a no-op
    // until something modifies it.
    speed *= player
        .attributes()
        .lock()
        .required_value(vanilla_attributes::BLOCK_BREAK_SPEED) as f32;

    // Vanilla parity: `if (isEyeInFluid(WATER)) speed *= SUBMERGED_MINING_SPEED`.
    // The base value is 0.2, and Aqua Affinity is the modifier that lifts it
    // back to 1 -- which is the whole of the enchantment.
    if player.is_eye_in_water() {
        speed *= player
            .attributes()
            .lock()
            .required_value(vanilla_attributes::SUBMERGED_MINING_SPEED) as f32;
    }

    // Vanilla parity: the `if (!this.onGround()) speed /= 5.0F`, which is why
    // mining while falling or jumping takes five times as long.
    if !player.on_ground() {
        speed /= AIRBORNE_MINING_PENALTY;
    }

    speed
}

/// Returns the better of the two effects that speed digging up.
///
/// Vanilla parity: `MobEffectUtil.getDigSpeedAmplification`, guarded by
/// `hasDigSpeed`; `None` here is vanilla's "neither is active".
fn dig_speed_amplification(player: &Player) -> Option<i32> {
    let haste = player
        .mob_effect(vanilla_mob_effects::HASTE)
        .map(|active| active.amplifier());
    let conduit = player
        .mob_effect(vanilla_mob_effects::CONDUIT_POWER)
        .map(|active| active.amplifier());
    match (haste, conduit) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}

/// Drops loot for a destroyed block using its loot table.
fn drop_block_loot(
    player: &Player,
    world: &Arc<World>,
    pos: BlockPos,
    state: BlockStateId,
    tool: &ItemStack,
    block_entity: Option<&SharedBlockEntity>,
) {
    let luck = player
        .attributes()
        .lock()
        .get_value(vanilla_attributes::LUCK)
        .unwrap_or(0.0) as f32;

    let drops = BlockLootContext::new(world, pos)
        .with_luck(luck)
        .with_tool(tool)
        .with_block_entity(block_entity)
        .get_drops(state);

    // Spawn each dropped item using the player's world reference (Arc<World>)
    for item in drops {
        if !item.is_empty() {
            player.get_world().pop_resource(pos, item);
        }
    }

    BLOCK_BEHAVIORS
        .get_behavior(state.get_block())
        .spawn_after_break(state, world, pos, tool, true);
}

/// Runs the held item's veto on breaking `state`.
///
/// Vanilla parity: the `ItemStack.canDestroyBlock` guard opening
/// `ServerPlayerGameMode.destroyBlock`. Vanilla mutates the live stack; Foton
/// works on a copy so the hook never runs while the inventory lock is held,
/// and writes it back only when the item actually changed it.
fn item_can_destroy_block(
    player: &Player,
    world: &Arc<World>,
    pos: BlockPos,
    state: BlockStateId,
) -> bool {
    let mut held = {
        let inventory = player.inventory.lock();
        let item = inventory.get_item_in_hand(InteractionHand::MainHand);
        item.copy_with_count(item.count())
    };
    let before = held.clone();

    let allowed = ITEM_BEHAVIORS
        .get_behavior(held.item())
        .can_destroy_block(&mut held, state, world, pos, player);

    if held != before {
        player
            .inventory
            .lock()
            .set_item_in_hand(InteractionHand::MainHand, held);
    }
    allowed
}

#[cfg(test)]
mod mining_speed_tests {
    use foton_registry::data_components::vanilla_components::{ENCHANTMENTS, ItemEnchantments};
    use foton_registry::init_vanilla_registry;
    use foton_registry::items::ItemRef;
    use foton_registry::vanilla_items;
    use foton_utils::Identifier;

    use super::*;
    use crate::entity::{EntityFluidContact, MobEffectInstance};
    use crate::inventory::equipment::EntityEquipment as _;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world};

    /// A tool speed that is easy to read the multipliers off.
    const BASE: f32 = 10.0;

    fn digger(key: &'static str) -> Arc<Player> {
        init_vanilla_registry();
        let world = fresh_test_world(key);
        let player = TestPlayerBuilder::new(world, "Digger", 1).build();
        player.set_on_ground(true);
        player
    }

    fn close(left: f32, right: f32) -> bool {
        (left - right).abs() <= right.abs() * 1.0e-5
    }

    #[test]
    fn haste_and_conduit_power_do_not_stack() {
        let player = digger("mining_speed_haste");
        assert!(close(apply_mining_speed_modifiers(&player, BASE), BASE));

        // Haste II is `1 + (1 + 1) * 0.2`.
        assert!(player.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::HASTE,
            200,
            1,
        )));
        assert!(close(
            apply_mining_speed_modifiers(&player, BASE),
            BASE * 1.4
        ));

        // Conduit Power III alongside it takes the better amplifier, not the
        // sum: `1 + (2 + 1) * 0.2`, where adding them would give `1.8`.
        assert!(player.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::CONDUIT_POWER,
            200,
            2,
        )));
        assert!(close(
            apply_mining_speed_modifiers(&player, BASE),
            BASE * 1.6
        ));
    }

    /// The modifiers are worth nothing if the breaking path never asks for
    /// them, so this one goes the long way round through `get_destroy_progress`.
    #[test]
    fn the_breaking_path_asks_for_the_modifiers() {
        let player = digger("mining_speed_reaches_breaking");
        let stone = vanilla_blocks::STONE.default_state();
        let unhindered = get_destroy_progress(&player, stone);
        assert!(unhindered > 0.0, "a bare hand still chips at stone");

        assert!(player.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::MINING_FATIGUE,
            200,
            0,
        )));
        assert!(close(
            get_destroy_progress(&player, stone),
            unhindered * 0.3
        ));
    }

    /// The fatigue steps are a table rather than a curve, and getting them
    /// wrong is the difference between a slow dig and a hopeless one.
    #[test]
    fn mining_fatigue_walks_its_own_table() {
        for (amplifier, expected) in [(0, 0.3_f32), (1, 0.09), (2, 0.0027), (3, 0.000_81)] {
            let player = digger(match amplifier {
                0 => "mining_speed_fatigue_0",
                1 => "mining_speed_fatigue_1",
                2 => "mining_speed_fatigue_2",
                _ => "mining_speed_fatigue_3",
            });
            assert!(player.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::MINING_FATIGUE,
                200,
                amplifier,
            )));
            assert!(
                close(apply_mining_speed_modifiers(&player, BASE), BASE * expected),
                "fatigue {amplifier} should scale by {expected}"
            );
        }
    }

    #[test]
    fn mining_off_the_ground_takes_five_times_as_long() {
        let player = digger("mining_speed_airborne");
        player.set_on_ground(false);
        assert!(close(
            apply_mining_speed_modifiers(&player, BASE),
            BASE / 5.0
        ));
    }

    fn enchanted(item: ItemRef, enchantment: Identifier, level: u32) -> ItemStack {
        let mut levels = ItemEnchantments::empty();
        levels.set(enchantment, level);
        let mut stack = ItemStack::new(item);
        stack.set(ENCHANTMENTS, levels);
        stack
    }

    /// Equips a slot the way the tick does.
    ///
    /// `LivingEntity::detect_equipment_updates` is what
    /// `tick_living_entity` calls, and it is the only thing that reaches
    /// `refresh_equipment_attribute_modifiers` in game -- so these tests come
    /// in through it rather than calling the refresh directly.
    fn equip(player: &Arc<Player>, slot: EquipmentSlot, stack: ItemStack) {
        player.inventory.lock().set(slot, stack);
        LivingEntity::detect_equipment_updates(player.as_ref());
    }

    /// The whole of Efficiency, from the item to the dig.
    ///
    /// This is the first enchantment nearly every player puts on anything, and
    /// it was inert: the enchantment landed on the pickaxe, showed in the
    /// tooltip, and `mining_speed_from_attributes` threw the attribute away.
    /// The route runs enchantment -> `MINING_EFFICIENCY` modifier ->
    /// `Player.getDestroySpeed`, so this comes in at `get_destroy_progress`,
    /// which is the call the breaking path makes.
    #[test]
    fn an_efficiency_pickaxe_digs_faster_than_a_plain_one() {
        let player = digger("mining_speed_efficiency");
        let stone = vanilla_blocks::STONE.default_state();

        equip(
            &player,
            EquipmentSlot::MainHand,
            ItemStack::new(&vanilla_items::DIAMOND_PICKAXE),
        );
        let plain = get_destroy_progress(&player, stone);
        assert!(plain > 0.0, "a diamond pickaxe should dig stone");

        // Vanilla `efficiency.json` is `levels_squared` with `added: 1`, so
        // level five adds 26 to a diamond pickaxe's speed of 8.
        equip(
            &player,
            EquipmentSlot::MainHand,
            enchanted(
                &vanilla_items::DIAMOND_PICKAXE,
                Identifier::vanilla_static("efficiency"),
                5,
            ),
        );
        let enchanted_progress = get_destroy_progress(&player, stone);

        assert!(
            close(enchanted_progress, plain * (8.0 + 26.0) / 8.0),
            "Efficiency V should take a diamond pickaxe from 8 to 34: \
             {plain} -> {enchanted_progress}"
        );
    }

    /// Efficiency is worth nothing on a block the tool is not for.
    ///
    /// Vanilla guards the term with `if (speed > 1.0F)`, and dropping the guard
    /// would let an Efficiency pickaxe tear through wool.
    #[test]
    fn efficiency_does_nothing_for_a_tool_that_does_not_apply() {
        let player = digger("mining_speed_efficiency_wrong_tool");

        equip(
            &player,
            EquipmentSlot::MainHand,
            enchanted(
                &vanilla_items::DIAMOND_PICKAXE,
                Identifier::vanilla_static("efficiency"),
                5,
            ),
        );
        let wool = vanilla_blocks::WHITE_WOOL.default_state();
        let with_pickaxe = get_destroy_progress(&player, wool);

        equip(&player, EquipmentSlot::MainHand, ItemStack::empty());
        let bare_handed = get_destroy_progress(&player, wool);

        assert!(
            close(with_pickaxe, bare_handed),
            "a pickaxe is speed 1 on wool, so Efficiency's `> 1.0` guard should \
             hold it back: {with_pickaxe} vs {bare_handed}"
        );
    }

    /// The whole of Aqua Affinity.
    ///
    /// `SUBMERGED_MINING_SPEED` starts at 0.2, which is the five-times penalty
    /// for mining with your head under water; the enchantment's
    /// `add_multiplied_total` of 4 takes it back to 1.
    #[test]
    fn aqua_affinity_lifts_the_underwater_penalty() {
        let player = digger("mining_speed_aqua_affinity");
        player
            .base()
            .set_fluid_contact(EntityFluidContact::from_parts(2.0, 0.0, true, false));
        assert!(
            player.is_eye_in_water(),
            "test setup failed: the assertions below only mean something with \
             the digger's head under water"
        );

        let drowning = apply_mining_speed_modifiers(&player, BASE);
        assert!(
            close(drowning, BASE * 0.2),
            "a bare head underwater digs at a fifth speed: {drowning}"
        );

        equip(
            &player,
            EquipmentSlot::Head,
            enchanted(
                &vanilla_items::DIAMOND_HELMET,
                Identifier::vanilla_static("aqua_affinity"),
                1,
            ),
        );

        assert!(
            close(apply_mining_speed_modifiers(&player, BASE), BASE),
            "Aqua Affinity should cancel the penalty entirely"
        );
    }

    /// One modifier per slot, not one per enchantment.
    ///
    /// `EnchantmentAttributeEffect.getModifier` suffixes the modifier id with
    /// the slot, and without that suffix a second armor piece carrying the same
    /// enchantment overwrites the first instead of adding to it.
    #[test]
    fn the_same_enchantment_on_two_slots_installs_two_modifiers() {
        let player = digger("enchantment_attributes_per_slot");
        let base = player
            .attributes()
            .lock()
            .required_value(vanilla_attributes::EXPLOSION_KNOCKBACK_RESISTANCE);

        for (slot, item) in [
            (EquipmentSlot::Head, &vanilla_items::DIAMOND_HELMET),
            (EquipmentSlot::Chest, &vanilla_items::DIAMOND_CHESTPLATE),
        ] {
            equip(
                &player,
                slot,
                enchanted(item, Identifier::vanilla_static("blast_protection"), 1),
            );
        }

        // `blast_protection.json` is `add_value` of 0.15 per level.
        let both = player
            .attributes()
            .lock()
            .required_value(vanilla_attributes::EXPLOSION_KNOCKBACK_RESISTANCE);
        assert!(
            close((both - base) as f32, 0.30),
            "two Blast Protection pieces should add 0.15 twice, not once: \
             {base} -> {both}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foton_registry::blocks::properties::DoubleBlockHalf;
    use foton_registry::init_vanilla_registry;
    use foton_utils::ChunkPos;

    use foton_registry::blocks::BlockRef;
    use foton_registry::blocks::properties::BlockStateProperties;
    use foton_registry::items::ItemRef;
    use foton_registry::vanilla_items;
    use foton_utils::{Downcast as _, WorldAabb};

    use crate::behavior::init_behaviors;
    use crate::entity::entities::ItemEntity;
    use crate::inventory::container::Container as _;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    /// Every item stack lying in the block at `pos`.
    fn items_dropped_at(world: &Arc<World>, pos: BlockPos) -> Vec<ItemStack> {
        let aabb = WorldAabb::new(
            f64::from(pos.x()) - 1.0,
            f64::from(pos.y()) - 1.0,
            f64::from(pos.z()) - 1.0,
            f64::from(pos.x()) + 2.0,
            f64::from(pos.y()) + 2.0,
            f64::from(pos.z()) + 2.0,
        );
        world
            .get_entities_in_aabb(&aabb)
            .iter()
            .filter_map(|entity| {
                entity
                    .downcast_ref::<ItemEntity>()
                    .map(ItemEntity::get_item)
            })
            .collect()
    }

    /// Breaks one half of a two-block plant and reports what fell.
    ///
    /// A double plant must pay out once whichever half is struck: its loot is
    /// rolled while both halves still stand, and the half that falls with it
    /// must not be paid for again.
    fn break_half_of(
        plant: BlockRef,
        held: ItemRef,
        broken: DoubleBlockHalf,
        key: &'static str,
    ) -> Vec<ItemStack> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let lower_pos = BlockPos::new(8, 64, 8);
        let half = &BlockStateProperties::DOUBLE_BLOCK_HALF;
        let lower = plant
            .default_state()
            .set_value(half, DoubleBlockHalf::Lower);
        let upper = plant
            .default_state()
            .set_value(half, DoubleBlockHalf::Upper);
        world.set_block(lower_pos, lower, UpdateFlags::UPDATE_ALL);
        world.set_block(lower_pos.above(), upper, UpdateFlags::UPDATE_ALL);

        let player = TestPlayerBuilder::new(Arc::clone(&world), "Mower", 4_242).build();
        player.inventory.lock().set_item(0, ItemStack::new(held));

        let broken_pos = match broken {
            DoubleBlockHalf::Lower => lower_pos,
            DoubleBlockHalf::Upper => lower_pos.above(),
        };
        assert!(BlockBreakingManager::new().destroy_block(&player, &world, broken_pos));

        items_dropped_at(&world, lower_pos)
    }

    fn assert_sheared_fern(broken: DoubleBlockHalf, key: &'static str) {
        let dropped = break_half_of(
            &vanilla_blocks::LARGE_FERN,
            &vanilla_items::SHEARS,
            broken,
            key,
        );
        assert_eq!(dropped.len(), 1, "expected one fern stack, got {dropped:?}");
        assert!(dropped[0].is(&vanilla_items::FERN));
        assert_eq!(dropped[0].count(), 2);
    }

    #[test]
    fn shearing_the_lower_half_of_a_large_fern_pays_out_once() {
        assert_sheared_fern(DoubleBlockHalf::Lower, "large_fern_lower_break");
    }

    #[test]
    fn shearing_the_upper_half_of_a_large_fern_pays_out_once() {
        assert_sheared_fern(DoubleBlockHalf::Upper, "large_fern_upper_break");
    }

    /// The pickaxe that dies on its last block says so.
    ///
    /// Vanilla's `ItemStack.hurtAndBreak` carries the miner and announces the
    /// break itself. Foton's item stacks cannot reach one, and `mine_block`
    /// returns vanilla's "the item handled it" boolean rather than "it broke",
    /// so the tool used to vanish from the hand with no snap and no splinters.
    #[test]
    fn a_pickaxe_that_breaks_on_its_last_block_announces_it() {
        use std::io::Cursor;

        use foton_protocol::packet_traits::EncodedPacket;
        use foton_registry::packets::play::C_ENTITY_EVENT;
        use foton_utils::codec::VarInt;
        use foton_utils::entity_events::EntityStatus;
        use foton_utils::locks::SyncMutex;
        use foton_utils::serial::ReadFrom as _;

        use crate::chunk::player_chunk_view::PlayerChunkView;
        use crate::entity::next_entity_id;
        use crate::player::{PlayerConnection, ResetReason};
        use crate::test_support::RecordingConnection;

        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("pickaxe_breaks_while_mining");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let pos = BlockPos::new(8, 64, 8);
        world.set_block(
            pos,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL,
        );

        let sent = Arc::new(SyncMutex::new(Vec::new()));
        let connection = Arc::new(PlayerConnection::Other(Box::new(RecordingConnection::new(
            Arc::clone(&sent),
        ))));
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Miner", next_entity_id())
            .connection(connection)
            .build();
        // The break is broadcast to whoever is tracking the chunk, so the miner
        // only hears it once they are one of them.
        assert!(world.add_player(Arc::clone(&player), ResetReason::InitialJoin));
        let _ = player.mark_joined_world();
        player.set_client_loaded(true);
        player
            .chunk_sender
            .lock()
            .mark_chunk_sent_for_test(ChunkPos::new(0, 0));
        world
            .player_area_map
            .on_player_join(&player, &PlayerChunkView::new(ChunkPos::new(0, 0), 2));

        let mut pickaxe = ItemStack::new(&vanilla_items::IRON_PICKAXE);
        pickaxe.set_damage_value(pickaxe.get_max_damage() - 1);
        player.inventory.lock().set_selected_item(pickaxe);
        sent.lock().clear();

        assert!(BlockBreakingManager::new().destroy_block(&player, &world, pos));

        assert!(
            player.inventory.lock().get_selected_item().is_empty(),
            "the pickaxe was one point from the end, so this block should have finished it"
        );

        let breaks: Vec<i32> = sent
            .lock()
            .iter()
            .filter_map(|packet: &EncodedPacket| {
                let mut cursor = Cursor::new(packet.encoded_data.as_slice());
                VarInt::read(&mut cursor).ok()?;
                if VarInt::read(&mut cursor).ok()?.0 != C_ENTITY_EVENT {
                    return None;
                }
                let entity_id = i32::read(&mut cursor).ok()?;
                let status = VarInt::read(&mut cursor).ok()?;
                (entity_id == player.id()).then_some(status.0)
            })
            .collect();
        assert!(
            breaks.contains(&(EntityStatus::MainhandBreak as i32)),
            "the break should be broadcast, got {breaks:?}"
        );
    }

    /// A pitcher plant's table has no `location_check` to stop a second
    /// payout, so it is the one that notices if the plant is rolled both
    /// before and after the block is taken away.
    #[test]
    fn breaking_a_pitcher_plant_pays_out_once() {
        let dropped = break_half_of(
            &vanilla_blocks::PITCHER_PLANT,
            &vanilla_items::STICK,
            DoubleBlockHalf::Lower,
            "pitcher_plant_break",
        );
        assert_eq!(
            dropped.len(),
            1,
            "expected one pitcher plant, got {dropped:?}"
        );
        assert!(dropped[0].is(&vanilla_items::PITCHER_PLANT));
        assert_eq!(dropped[0].count(), 1);
    }
}
