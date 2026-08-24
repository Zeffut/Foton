//! The evoker's three spells.
//!
//! Vanilla parity: `Evoker.EvokerSummonSpellGoal`, `Evoker.EvokerAttackSpellGoal`
//! and `Evoker.EvokerWololoSpellGoal`. Each is four numbers and a body over the
//! shared warmup-cast-cooldown shape in
//! [`crate::entity::ai::goal::SpellcasterUseSpellBase`].

use std::f64::consts::{PI, TAU};
use std::sync::Arc;

use glam::DVec3;
use steel_math::trig;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_game_rules::MOB_GRIEFING;
use steel_registry::{DyeColor, sound_events, vanilla_entities, vanilla_game_events};
use steel_utils::axis::Axis;
use steel_utils::{BlockPos, Direction, Downcast as _};

use super::EvokerEntity;
use crate::entity::ai::goal::{
    DEFAULT_CAST_WARMUP_TIME, Goal, GoalControls, SpellcasterUseSpellBase,
};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::entities::{EvokerFangsEntity, SheepEntity, VexEntity};
use crate::entity::spellcaster_illager::SpellcasterIllager;
use crate::entity::{
    EntitySpawnReason, IllagerSpell, LivingEntity, Mob, PathfinderMob, SharedEntity, next_entity_id,
};
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, World};

/// How far the evoker counts its own vexes.
///
/// Vanilla parity: the `range(16.0)` and `inflate(16.0)` of the summon goal.
const VEX_COUNT_RANGE: f64 = 16.0;

/// Upper bound on the roll the vex count is compared against.
///
/// Vanilla parity: the `random.nextInt(8) + 1 > vexes` of `canUse`, which is
/// why an evoker with a full escort only rarely summons more.
const VEX_ROLL_BOUND: i32 = 8;

/// How many vexes one summon brings.
///
/// Vanilla parity: the three-iteration loop of `performSpellCasting`.
const VEX_SUMMON_COUNT: i32 = 3;

/// Half-width of the square a summoned vex appears in.
///
/// Vanilla parity: the two offsets of `performSpellCasting`, each a
/// `-2 + nextInt(5)`.
const VEX_SPAWN_SPREAD: i32 = 2;

/// How far above the evoker a summoned vex appears.
const VEX_SPAWN_HEIGHT: i32 = 1;

/// Shortest a summoned vex lives, in seconds.
///
/// Vanilla parity: the `20 * (30 + nextInt(90))` limited life, which is what
/// makes an evoker's escort run out rather than accumulate.
const VEX_LIFE_MIN_SECONDS: i32 = 30;

/// Width of the random part of a summoned vex's life, in seconds.
const VEX_LIFE_SPREAD_SECONDS: i32 = 90;

/// Ticks in a second.
const TICKS_PER_SECOND: i32 = 20;

/// Ticks the evoker keeps its hands up while summoning.
const SUMMON_CASTING_TIME: i32 = 100;

/// Ticks between two summons.
const SUMMON_CASTING_INTERVAL: i32 = 340;

/// Ticks the evoker keeps its hands up while calling fangs.
const FANGS_CASTING_TIME: i32 = 40;

/// Ticks between two fang lines.
const FANGS_CASTING_INTERVAL: i32 = 100;

/// Squared distance inside which the fangs come up in two rings instead of a
/// line.
///
/// Vanilla parity: the `distanceToSqr(target) < 9.0` of `performSpellCasting`.
const FANGS_RING_DISTANCE_SQR: f64 = 9.0;

/// Fangs in the inner ring.
const FANGS_INNER_RING_COUNT: i32 = 5;

/// Radius of the inner ring.
const FANGS_INNER_RING_RADIUS: f64 = 1.5;

/// Fangs in the outer ring.
const FANGS_OUTER_RING_COUNT: i32 = 8;

/// Radius of the outer ring.
const FANGS_OUTER_RING_RADIUS: f64 = 2.5;

/// Warmup the outer ring rises with, so it follows the inner one.
const FANGS_OUTER_RING_DELAY: i32 = 3;

/// Fangs in a line.
const FANGS_LINE_COUNT: i32 = 16;

/// Spacing between two fangs in a line.
const FANGS_LINE_SPACING: f64 = 1.25;

/// Warmup the evoker takes before recoloring a sheep.
const WOLOLO_CAST_WARMUP_TIME: i32 = 40;

/// Ticks the evoker keeps its hands up while recoloring.
const WOLOLO_CASTING_TIME: i32 = 60;

/// Ticks between two recolorings.
const WOLOLO_CASTING_INTERVAL: i32 = 140;

/// How far the evoker looks for a blue sheep, horizontally.
const WOLOLO_SEARCH_RANGE: f64 = 16.0;

/// How far the evoker looks for a blue sheep, vertically.
const WOLOLO_SEARCH_HEIGHT: f64 = 4.0;

/// Summons vexes.
///
/// Vanilla parity: `Evoker.EvokerSummonSpellGoal`.
///
pub(super) struct EvokerSummonSpellGoal {
    base: SpellcasterUseSpellBase,
    /// Who counts as one of this evoker's vexes.
    vex_count_targeting: TargetingConditions,
}

impl EvokerSummonSpellGoal {
    /// Creates the goal.
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            base: SpellcasterUseSpellBase::new(
                IllagerSpell::SummonVex,
                DEFAULT_CAST_WARMUP_TIME,
                SUMMON_CASTING_TIME,
                SUMMON_CASTING_INTERVAL,
                Some(&sound_events::ENTITY_EVOKER_PREPARE_SUMMON),
            ),
            vex_count_targeting: TargetingConditions::for_non_combat()
                .range(VEX_COUNT_RANGE)
                .ignore_line_of_sight()
                .ignore_invisibility_testing(),
        }
    }

    /// Counts the vexes already around the evoker.
    ///
    /// Vanilla parity: the `getNearbyEntities(Vex.class, ..)` of `canUse`.
    fn nearby_vexes(&self, mob: &dyn PathfinderMob) -> usize {
        let Some(world) = mob.level() else {
            return 0;
        };
        let search_box = mob.bounding_box().inflate(VEX_COUNT_RANGE);
        let level = world.as_ref();
        world
            .get_entities_in_aabb_matching(&search_box, |entity| {
                entity.entity_type() == &vanilla_entities::VEX
                    && entity.as_living_entity().is_some_and(|living| {
                        self.vex_count_targeting.test(level, Some(mob), living)
                    })
            })
            .len()
    }
}

impl Goal for EvokerSummonSpellGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if !self.base.can_use(mob) {
            return false;
        }
        let vexes = i32::try_from(self.nearby_vexes(mob)).unwrap_or(i32::MAX);
        rand::random_range(0..VEX_ROLL_BOUND) + 1 > vexes
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.base.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.base.start(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.base.tick(mob, summon_vexes);
    }
}

/// Puts three vexes in the air around the evoker.
///
/// Vanilla parity: `EvokerSummonSpellGoal.performSpellCasting`. Each vex is
/// bound to the spot it appeared at, so the escort hangs over the evoker
/// rather than scattering, and each is given thirty seconds to two minutes to
/// live. Vanilla also copies the evoker's scoreboard team onto the vex; Steel
/// puts no entity on a team.
fn summon_vexes(mob: &dyn PathfinderMob) {
    let Some(world) = mob.level() else {
        return;
    };
    let origin = mob.block_position();

    for _ in 0..VEX_SUMMON_COUNT {
        let pos = origin.offset(
            rand::random_range(-VEX_SPAWN_SPREAD..=VEX_SPAWN_SPREAD),
            VEX_SPAWN_HEIGHT,
            rand::random_range(-VEX_SPAWN_SPREAD..=VEX_SPAWN_SPREAD),
        );
        let (x, y, z) = pos.get_bottom_center();
        let spawn = DVec3::new(x, y, z);

        let vex = Arc::new(VexEntity::new(
            &vanilla_entities::VEX,
            next_entity_id(),
            spawn,
            Arc::downgrade(&world),
        ));
        vex.finalize_spawn(&world, EntitySpawnReason::MobSummoned, None);
        vex.set_owner(mob.as_entity_event_source());
        vex.set_bound_origin(Some(pos));
        vex.set_limited_life(
            TICKS_PER_SECOND
                * (VEX_LIFE_MIN_SECONDS + rand::random_range(0..VEX_LIFE_SPREAD_SECONDS)),
        );

        let entity: SharedEntity = vex;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("evoker failed to summon a vex: {error}");
            continue;
        }
        world.game_event_at(
            &vanilla_game_events::ENTITY_PLACE,
            spawn,
            &GameEventContext::new(Some(mob.as_entity_event_source()), None),
        );
    }
}

/// Calls a line or two rings of fangs out of the ground.
///
/// Vanilla parity: `Evoker.EvokerAttackSpellGoal`.
pub(super) struct EvokerAttackSpellGoal {
    base: SpellcasterUseSpellBase,
}

impl EvokerAttackSpellGoal {
    /// Creates the goal.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            base: SpellcasterUseSpellBase::new(
                IllagerSpell::Fangs,
                DEFAULT_CAST_WARMUP_TIME,
                FANGS_CASTING_TIME,
                FANGS_CASTING_INTERVAL,
                Some(&sound_events::ENTITY_EVOKER_PREPARE_ATTACK),
            ),
        }
    }
}

impl Goal for EvokerAttackSpellGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.base.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.base.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.base.start(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.base.tick(mob, cast_fangs);
    }
}

/// Lays out the fangs.
///
/// Vanilla parity: `EvokerAttackSpellGoal.performSpellCasting`. Close in, the
/// fangs come up in two rings around the evoker; further out, in a line walking
/// towards the target, each one three ticks behind the last.
fn cast_fangs(mob: &dyn PathfinderMob) {
    let Some(target) = mob.target() else {
        return;
    };
    let Some(world) = mob.level() else {
        return;
    };

    let position = mob.position();
    let target_position = target.position();
    let min_y = target_position.y.min(position.y);
    let max_y = target_position.y.max(position.y) + 1.0;
    let angle_to_target =
        (target_position.z - position.z).atan2(target_position.x - position.x) as f32;

    if position.distance_squared(target_position) < FANGS_RING_DISTANCE_SQR {
        for i in 0..FANGS_INNER_RING_COUNT {
            let angle = f64::from(i).mul_add(PI * 0.4, f64::from(angle_to_target)) as f32;
            place_fangs(
                mob,
                &world,
                ring_point(position, angle, FANGS_INNER_RING_RADIUS),
                min_y,
                max_y,
                angle,
                0,
            );
        }
        for i in 0..FANGS_OUTER_RING_COUNT {
            let angle =
                f64::from(i).mul_add(TAU / 8.0, f64::from(angle_to_target) + TAU / 5.0) as f32;
            place_fangs(
                mob,
                &world,
                ring_point(position, angle, FANGS_OUTER_RING_RADIUS),
                min_y,
                max_y,
                angle,
                FANGS_OUTER_RING_DELAY,
            );
        }
        return;
    }

    for i in 0..FANGS_LINE_COUNT {
        let reach = FANGS_LINE_SPACING * f64::from(i + 1);
        place_fangs(
            mob,
            &world,
            ring_point(position, angle_to_target, reach),
            min_y,
            max_y,
            angle_to_target,
            i,
        );
    }
}

/// Returns the point `radius` away from `origin` at `angle`.
fn ring_point(origin: DVec3, angle: f32, radius: f64) -> (f64, f64) {
    let cos = f64::from(trig::cos(f64::from(angle)));
    let sin = f64::from(trig::sin(f64::from(angle)));
    (cos.mul_add(radius, origin.x), sin.mul_add(radius, origin.z))
}

/// Drops one pair of fangs onto the first solid surface below `max_y`.
///
/// Vanilla parity: `EvokerAttackSpellGoal.createSpellEntity`. Fangs only rise
/// out of a face that could be stood on, which is why a fang line stops at the
/// edge of a hole rather than continuing into it.
fn place_fangs(
    mob: &dyn PathfinderMob,
    world: &Arc<World>,
    (x, z): (f64, f64),
    min_y: f64,
    max_y: f64,
    angle: f32,
    delay_ticks: i32,
) {
    let mut pos = BlockPos::containing(x, max_y, z);
    let mut top_offset = 0.0;
    let mut found = false;
    let floor = min_y.floor() as i32 - 1;

    while pos.y() >= floor {
        let below = pos.below();
        let below_state = world.get_block_state(below);
        if world.is_face_sturdy(below_state, below, Direction::Up) {
            let state = world.get_block_state(pos);
            if !state.is_air() {
                let shape = state.get_collision_shape_at(pos);
                if !shape.is_empty() {
                    top_offset = shape.max(Axis::Y);
                }
            }
            found = true;
            break;
        }
        pos = pos.below();
    }

    if !found {
        return;
    }

    let spawn = DVec3::new(x, f64::from(pos.y()) + top_offset, z);
    let fangs = Arc::new(EvokerFangsEntity::new(
        &vanilla_entities::EVOKER_FANGS,
        next_entity_id(),
        spawn,
        Arc::downgrade(world),
    ));
    fangs.place(spawn, angle, delay_ticks);
    fangs.set_owner_uuid(Some(mob.uuid()));

    let entity: SharedEntity = fangs;
    if let Err(error) = world.try_add_entity(entity) {
        log::debug!("evoker failed to raise its fangs: {error}");
        return;
    }

    world.game_event_at(
        &vanilla_game_events::ENTITY_PLACE,
        spawn,
        &GameEventContext::new(Some(mob.as_entity_event_source()), None),
    );
}

/// Turns a blue sheep red.
///
/// Vanilla parity: `Evoker.EvokerWololoSpellGoal`. The only spell an evoker
/// casts with nothing to fight, and the only one whose target is not the thing
/// it is attacking, which is why it overrides the whole gate.
pub(super) struct EvokerWololoSpellGoal {
    base: SpellcasterUseSpellBase,
    /// Which sheep is worth recoloring.
    wololo_targeting: TargetingConditions,
}

impl EvokerWololoSpellGoal {
    /// Creates the goal.
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            base: SpellcasterUseSpellBase::new(
                IllagerSpell::Wololo,
                WOLOLO_CAST_WARMUP_TIME,
                WOLOLO_CASTING_TIME,
                WOLOLO_CASTING_INTERVAL,
                Some(&sound_events::ENTITY_EVOKER_PREPARE_WOLOLO),
            ),
            wololo_targeting: TargetingConditions::for_non_combat()
                .range(WOLOLO_SEARCH_RANGE)
                .selector(|_, target, _| {
                    target
                        .downcast_ref::<SheepEntity>()
                        .is_some_and(|sheep| sheep.color() == DyeColor::Blue)
                }),
        }
    }
}

impl Goal for EvokerWololoSpellGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(evoker) = mob.downcast_ref::<EvokerEntity>() else {
            return false;
        };
        if mob.target().is_some() || evoker.is_casting_spell() || !self.base.is_off_cooldown(mob) {
            return false;
        }
        let Some(world) = mob.level() else {
            return false;
        };
        if !world.get_game_rule(&MOB_GRIEFING) {
            return false;
        }

        let search_box = mob.bounding_box().inflate_xyz(
            WOLOLO_SEARCH_RANGE,
            WOLOLO_SEARCH_HEIGHT,
            WOLOLO_SEARCH_RANGE,
        );
        let level = world.as_ref();
        let sheep = world.get_entities_in_aabb_matching(&search_box, |entity| {
            entity
                .as_living_entity()
                .is_some_and(|living| self.wololo_targeting.test(level, Some(mob), living))
        });
        if sheep.is_empty() {
            return false;
        }

        let picked = &sheep[rand::random_range(0..sheep.len())];
        evoker.set_wololo_target(Some(picked));
        true
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.downcast_ref::<EvokerEntity>()
            .is_some_and(|evoker| evoker.wololo_target().is_some())
            && self.base.attack_warmup_delay() > 0
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.base.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(evoker) = mob.downcast_ref::<EvokerEntity>() {
            evoker.set_wololo_target(None);
        }
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.base.tick(mob, |mob| {
            let Some(evoker) = mob.downcast_ref::<EvokerEntity>() else {
                return;
            };
            let Some(target) = evoker.wololo_target() else {
                return;
            };
            let Some(sheep) = target.downcast_ref::<SheepEntity>() else {
                return;
            };
            if LivingEntity::is_alive(sheep) {
                sheep.set_color(DyeColor::Red);
            }
        });
    }
}
