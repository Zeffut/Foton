//! The goal that puts a cat on its sleeping owner, and the present it leaves.
//!
//! Vanilla parity: `Cat.CatRelaxOnOwnerGoal`, an inner class of `Cat`.

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_loot_tables;
use steel_utils::{BlockPos, Downcast as _, WorldAabb};

use super::CatEntity;
use crate::entity::ai::goal::{Goal, GoalControls, reduced_tick_delay};
use crate::entity::living_entity::gift_loot_items_with_rng;
use crate::entity::{Entity, LivingEntity, Mob, PathfinderMob, TamableAnimal};
use crate::player::Player;

/// Speed the cat walks to the bed at.
///
/// Vanilla parity: the `1.1F` of `CatRelaxOnOwnerGoal`.
const APPROACH_SPEED: f64 = 1.1;

/// How far the cat may be from its owner and still want the bed.
///
/// Vanilla parity: the `distanceToSqr(ownerPlayer) > 100.0` guard.
const OWNER_SEARCH_DISTANCE_SQR: f64 = 100.0;

/// How close the cat has to be to count as on the bed.
///
/// Vanilla parity: the `distanceToSqr(ownerPlayer) < 2.5` of the tick.
const ON_BED_DISTANCE_SQR: f64 = 2.5;

/// How long the cat settles before it lies down properly.
///
/// Vanilla parity: the `adjustedTickDelay(16)` of the tick.
const SETTLE_TICKS: i32 = 16;

/// How near another cat has to be to have claimed the spot.
///
/// Vanilla parity: the `inflate(2.0)` of `spaceIsOccupied`.
const OCCUPIED_RADIUS: f64 = 2.0;

/// How far the cat may wander before dropping the morning gift.
///
/// Vanilla parity: the `nextInt(11) - 5` and `nextInt(5) - 2` of
/// `giveMorningGift`.
const GIFT_TELEPORT_HORIZONTAL_RANGE: i32 = 5;
const GIFT_TELEPORT_VERTICAL_RANGE: i32 = 2;

/// Sends a tamed cat to sleep on its owner's bed, and rewards them for it.
pub(super) struct CatRelaxOnOwnerGoal {
    goal_pos: Option<BlockPos>,
    on_bed_ticks: i32,
}

impl CatRelaxOnOwnerGoal {
    pub(super) const fn new() -> Self {
        Self {
            goal_pos: None,
            on_bed_ticks: 0,
        }
    }

    /// Vanilla parity: `CatRelaxOnOwnerGoal.spaceIsOccupied`.
    fn space_is_occupied(cat: &CatEntity, goal_pos: BlockPos) -> bool {
        let Some(world) = cat.level() else {
            return false;
        };

        let (x, y, z) = goal_pos.get_center();
        let search = WorldAabb::new(x - 0.5, y - 0.5, z - 0.5, x + 0.5, y + 0.5, z + 0.5)
            .inflate(OCCUPIED_RADIUS);
        let cat_uuid = cat.uuid();

        world.has_entity_in_aabb_matching(&search, |entity| {
            if entity.uuid() == cat_uuid {
                return false;
            }
            entity
                .downcast_ref::<CatEntity>()
                .is_some_and(|other| other.is_lying() || other.is_relax_state_one())
        })
    }

    /// Vanilla parity: `CatRelaxOnOwnerGoal.giveMorningGift`.
    fn give_morning_gift(cat: &CatEntity) {
        let Some(world) = cat.level() else {
            return;
        };

        let start = if cat.is_leashed() {
            cat.leash_holder()
                .map_or_else(|| cat.block_position(), |holder| holder.block_position())
        } else {
            cat.block_position()
        };
        let target = DVec3::new(
            f64::from(
                start.x()
                    + rand::random_range(
                        -GIFT_TELEPORT_HORIZONTAL_RANGE..=GIFT_TELEPORT_HORIZONTAL_RANGE,
                    ),
            ),
            f64::from(
                start.y()
                    + rand::random_range(
                        -GIFT_TELEPORT_VERTICAL_RANGE..=GIFT_TELEPORT_VERTICAL_RANGE,
                    ),
            ),
            f64::from(
                start.z()
                    + rand::random_range(
                        -GIFT_TELEPORT_HORIZONTAL_RANGE..=GIFT_TELEPORT_HORIZONTAL_RANGE,
                    ),
            ),
        );
        cat.random_teleport(target);

        let cat_pos = cat.block_position();
        let body_rot_radians = f64::from(cat.y_body_rot().to_radians());
        let drop_position = DVec3::new(
            f64::from(cat_pos.x()) - body_rot_radians.sin(),
            f64::from(cat_pos.y()),
            f64::from(cat_pos.z()) + body_rot_radians.cos(),
        );

        let mut rng = rand::rng();
        let gifts = gift_loot_items_with_rng(
            cat,
            &vanilla_loot_tables::GAMEPLAY_CAT_MORNING_GIFT,
            &mut rng,
        );
        for gift in gifts {
            world.spawn_item(drop_position, gift);
        }
    }
}

impl Goal for CatRelaxOnOwnerGoal {
    fn controls(&self) -> GoalControls {
        // Vanilla parity: `CatRelaxOnOwnerGoal` never calls `setFlags`, so it
        // claims nothing and runs alongside whatever else is moving the cat.
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(cat) = mob.downcast_ref::<CatEntity>() else {
            return false;
        };
        if !cat.is_tame() || cat.is_ordered_to_sit() {
            return false;
        }
        let Some(owner) = cat.owner() else {
            return false;
        };
        let Some(player) = owner.as_player() else {
            return false;
        };
        if !player.is_sleeping()
            || cat.position().distance_squared(player.position()) > OWNER_SEARCH_DISTANCE_SQR
        {
            return false;
        }
        let Some(world) = cat.level() else {
            return false;
        };

        let owner_pos = player.block_position();
        let owner_state = world.get_block_state(owner_pos);
        if !owner_state.get_block().has_tag(&BlockTag::BEDS) {
            return false;
        }

        let goal_pos = owner_state
            .try_get_value(&BlockStateProperties::HORIZONTAL_FACING)
            .map_or(owner_pos, |facing| owner_pos.relative(facing.opposite()));
        self.goal_pos = Some(goal_pos);
        !Self::space_is_occupied(cat, goal_pos)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(cat) = mob.downcast_ref::<CatEntity>() else {
            return false;
        };
        let Some(goal_pos) = self.goal_pos else {
            return false;
        };
        if !cat.is_tame() || cat.is_ordered_to_sit() {
            return false;
        }

        cat.owner()
            .and_then(|owner| owner.as_player().map(Player::is_sleeping))
            .unwrap_or(false)
            && !Self::space_is_occupied(cat, goal_pos)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(goal_pos) = self.goal_pos else {
            return;
        };
        if let Some(cat) = mob.downcast_ref::<CatEntity>() {
            cat.set_in_sitting_pose(false);
        }
        mob.move_to_pos(block_bottom_center(goal_pos), APPROACH_SPEED);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let Some(cat) = mob.downcast_ref::<CatEntity>() else {
            return;
        };
        cat.set_lying(false);

        let owner_slept_long_enough = cat
            .owner()
            .and_then(|owner| owner.as_player().map(Player::is_sleeping_long_enough))
            .unwrap_or(false);
        let gift_chance = cat
            .level()
            .map_or(0.0, |world| world.cat_waking_up_gift_chance());
        if owner_slept_long_enough && rand::random::<f32>() < gift_chance {
            Self::give_morning_gift(cat);
        }

        self.on_bed_ticks = 0;
        cat.set_relax_state_one(false);
        cat.mob_base().navigation().lock().stop();
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(cat) = mob.downcast_ref::<CatEntity>() else {
            return;
        };
        let Some(goal_pos) = self.goal_pos else {
            return;
        };
        let Some(owner) = cat.owner() else {
            return;
        };

        cat.set_in_sitting_pose(false);
        mob.move_to_pos(block_bottom_center(goal_pos), APPROACH_SPEED);

        if cat.position().distance_squared(owner.position()) >= ON_BED_DISTANCE_SQR {
            cat.set_lying(false);
            return;
        }

        self.on_bed_ticks += 1;
        if self.on_bed_ticks > reduced_tick_delay(SETTLE_TICKS) {
            cat.set_lying(true);
            cat.set_relax_state_one(false);
        } else {
            Mob::look_at(cat, owner.as_ref(), 45.0, 45.0);
            cat.set_relax_state_one(true);
        }
    }
}

fn block_bottom_center(pos: BlockPos) -> DVec3 {
    let (x, y, z) = pos.get_bottom_center();
    DVec3::new(x, y, z)
}
