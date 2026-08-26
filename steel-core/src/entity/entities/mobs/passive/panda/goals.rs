//! The panda's goals.
//!
//! Vanilla parity: the ten inner classes of `Panda`. Most of them are an
//! ordinary goal with `panda.canPerformAction()` bolted onto `canUse`, which
//! Rust expresses as a wrapper rather than a subclass; the four that are really
//! new -- sit, lie on back, roll and sneeze -- are written out.

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_blocks;
use steel_registry::vanilla_damage_type_tags::DamageTypeTag;
use steel_utils::{BlockPos, Downcast as _};

use crate::entity::ai::goal::{
    AvoidEntityGoal, BreedGoal, FloatGoal, FollowParentGoal, Goal, GoalControls, HurtByTargetGoal,
    LookAtPlayerGoal, MeleeAttackGoal, PanicGoal, RandomLookAroundGoal, TemptGoal,
    WaterAvoidingRandomStrollGoal, reduced_tick_delay,
};
use crate::entity::{
    AgeableMob, Entity as _, LivingEntity as _, Mob, MobBase, PathfinderMob, SharedEntity,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::world::LevelReader as _;

use super::{PandaEntity, TOTAL_UNHAPPY_TIME};

/// Vanilla parity: the `PandaPanicGoal(this, 2.0)` of `registerGoals`.
const PANIC_SPEED_MOD: f64 = 2.0;
/// Vanilla parity: the `PandaBreedGoal(this, 1.0)`.
const BREED_SPEED_MOD: f64 = 1.0;
/// Vanilla parity: the `PandaAttackGoal(this, 1.2F, true)`.
const ATTACK_SPEED_MOD: f64 = 1.2;
/// Vanilla parity: the `TemptGoal(this, 1.0, PANDA_FOOD, false)`.
const TEMPT_SPEED_MOD: f64 = 1.0;
/// Vanilla parity: the two `PandaAvoidGoal(this, ..., 2.0, 2.0)` speeds.
const AVOID_WALK_SPEED_MOD: f64 = 2.0;
const AVOID_SPRINT_SPEED_MOD: f64 = 2.0;
/// Vanilla parity: the `8.0F` a worried panda keeps from a player.
const AVOID_PLAYER_DIST: f32 = 8.0;
/// Vanilla parity: the `4.0F` it keeps from a monster.
const AVOID_MONSTER_DIST: f32 = 4.0;
/// Vanilla parity: the `PandaLookAtPlayerGoal(this, Player.class, 6.0F)`.
const LOOK_DISTANCE: f64 = 6.0;
/// Vanilla parity: the `FollowParentGoal(this, 1.25)`.
const FOLLOW_PARENT_SPEED_MOD: f64 = 1.25;
/// Vanilla parity: the `WaterAvoidingRandomStrollGoal(this, 1.0)`.
const STROLL_SPEED_MOD: f64 = 1.0;

/// Vanilla parity: the `BREED_TARGETING` range of `PandaBreedGoal`.
const BREED_TARGETING_RANGE: f64 = 8.0;
/// Vanilla parity: the `tickCount + 600` cooldown between complaints.
const UNHAPPY_COOLDOWN: i32 = 600;
/// Vanilla parity: the eight-block, three-high bamboo search of `canFindBamboo`.
const BAMBOO_SEARCH_RADIUS: i32 = 8;
const BAMBOO_SEARCH_HEIGHT: i32 = 3;

/// Vanilla parity: the `nextInt(400) == 1` a lazy panda flops on.
const LIE_ON_BACK_CHANCE: i32 = 400;
/// Vanilla parity: the `tickCount + 200` a panda waits before flopping again.
const LIE_ON_BACK_COOLDOWN: i32 = 200;
/// Vanilla parity: the `nextInt(600) != 1` and `nextInt(2000) != 1` that end
/// both the sit and the lie-on-back.
const REST_INTERRUPT_CHANCE: i32 = 600;
const REST_END_CHANCE: i32 = 2000;
/// Vanilla parity: the `inflate(6.0)` a sitting panda looks for bamboo in.
const SIT_ITEM_SEARCH_RANGE: f64 = 6.0;
/// Vanilla parity: the `inflate(8.0)` it then walks to bamboo in.
const SIT_ITEM_WALK_RANGE: f64 = 8.0;
/// Vanilla parity: the `1.2F` a panda walks to bamboo at.
const SIT_WALK_SPEED_MOD: f64 = 1.2;
/// Vanilla parity: the `nextInt(50) + 10` and `nextInt(150) + 10` seconds a
/// panda waits after dropping its bamboo -- a lazy one gets bored sooner.
const SIT_COOLDOWN_LAZY_BOUND: i32 = 50;
const SIT_COOLDOWN_BOUND: i32 = 150;
const SIT_COOLDOWN_BASE: i32 = 10;
const TICKS_PER_SECOND: i32 = 20;

/// Vanilla parity: the `nextInt(500) == 1` a weak cub sneezes on.
const WEAK_SNEEZE_CHANCE: i32 = 500;
/// Vanilla parity: the `nextInt(6000) == 1` any other cub sneezes on.
const SNEEZE_CHANCE: i32 = 6000;
/// Vanilla parity: the `nextInt(60) == 1` a playful panda rolls on.
const PLAYFUL_ROLL_CHANCE: i32 = 60;
/// Vanilla parity: the `nextInt(500) == 1` any roller rolls on.
const ROLL_CHANCE: i32 = 500;
/// Vanilla parity: the `Math.abs(dir) > 0.5` that picks the block ahead.
const ROLL_STEP_THRESHOLD: f32 = 0.5;

/// Vanilla parity: `Panda.registerGoals`.
pub fn register(mob_base: &MobBase) {
    {
        let mut goals = mob_base.goal_selector().lock();
        goals.add_goal(0, FloatGoal::new(mob_base));
        goals.add_goal(2, PandaPanicGoal::new());
        goals.add_goal(2, PandaBreedGoal::new());
        goals.add_goal(
            3,
            while_panda_can_act(MeleeAttackGoal::new(ATTACK_SPEED_MOD, true)),
        );
        goals.add_goal(
            4,
            TemptGoal::new(TEMPT_SPEED_MOD, PandaEntity::is_panda_food, false),
        );
        goals.add_goal(
            6,
            while_worried_panda_can_act(AvoidEntityGoal::with_selector(
                AVOID_PLAYER_DIST,
                AVOID_WALK_SPEED_MOD,
                AVOID_SPRINT_SPEED_MOD,
                |_, target, _| target.as_player().is_some() && !target.is_spectator(),
            )),
        );
        goals.add_goal(
            6,
            while_worried_panda_can_act(AvoidEntityGoal::with_selector(
                AVOID_MONSTER_DIST,
                AVOID_WALK_SPEED_MOD,
                AVOID_SPRINT_SPEED_MOD,
                |_, target, _| target.as_enemy().is_some() && !target.is_spectator(),
            )),
        );
        goals.add_goal(7, PandaSitGoal::new());
        goals.add_goal(8, PandaLieOnBackGoal::new());
        goals.add_goal(8, PandaSneezeGoal);
        goals.add_goal(
            9,
            LookAtPlayerGoal::new(LOOK_DISTANCE)
                .with_preset_target(|mob| {
                    with_panda(mob, PandaEntity::unhappy_look_target).flatten()
                })
                .with_extra_condition(|mob| {
                    with_panda(mob, PandaEntity::can_perform_action).unwrap_or(false)
                }),
        );
        goals.add_goal(10, RandomLookAroundGoal::new());
        goals.add_goal(12, PandaRollGoal);
        goals.add_goal(13, FollowParentGoal::new(FOLLOW_PARENT_SPEED_MOD));
        goals.add_goal(14, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MOD));
    }
    {
        let mut targets = mob_base.target_selector().lock();
        targets.add_goal(1, PandaHurtByTargetGoal::new());
    }
}

/// Runs `visit` on the mob when it is a panda.
fn with_panda<R>(mob: &dyn PathfinderMob, visit: impl FnOnce(&PandaEntity) -> R) -> Option<R> {
    mob.downcast_ref::<PandaEntity>().map(visit)
}

/// Vanilla parity: the `this.panda.canPerformAction() && super.canUse()` that
/// `PandaAttackGoal` and the two `PandaAvoidGoal`s all open with.
struct WhilePandaAllows<G: Goal> {
    inner: G,
    allows: fn(&PandaEntity) -> bool,
}

fn while_panda_can_act<G: Goal>(inner: G) -> WhilePandaAllows<G> {
    WhilePandaAllows {
        inner,
        allows: PandaEntity::can_perform_action,
    }
}

fn while_worried_panda_can_act<G: Goal>(inner: G) -> WhilePandaAllows<G> {
    WhilePandaAllows {
        inner,
        allows: |panda| panda.is_worried() && panda.can_perform_action(),
    }
}

impl<G: Goal> Goal for WhilePandaAllows<G> {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        with_panda(mob, self.allows).unwrap_or(false) && self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn is_interruptable(&self) -> bool {
        self.inner.is_interruptable()
    }

    fn is_panic_goal(&self) -> bool {
        self.inner.is_panic_goal()
    }

    fn is_tempt_goal(&self) -> bool {
        self.inner.is_tempt_goal()
    }

    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}

/// Vanilla parity: `Panda.PandaPanicGoal`, which lets a sitting panda sit
/// through whatever is happening.
struct PandaPanicGoal {
    inner: PanicGoal,
}

impl PandaPanicGoal {
    fn new() -> Self {
        Self {
            inner: PanicGoal::with_damage_types(
                PANIC_SPEED_MOD,
                DamageTypeTag::PANIC_ENVIRONMENTAL_CAUSES,
            ),
        }
    }
}

impl Goal for PandaPanicGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if with_panda(mob, PandaEntity::is_sitting).unwrap_or(false) {
            mob.mob_base().navigation().lock().stop();
            return false;
        }
        self.inner.can_continue_to_use(mob)
    }

    fn is_panic_goal(&self) -> bool {
        true
    }

    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}

/// Vanilla parity: `Panda.PandaBreedGoal`, which refuses to breed anywhere
/// there is no bamboo growing and whines about it.
struct PandaBreedGoal {
    inner: BreedGoal,
    unhappy_cooldown: i32,
}

impl PandaBreedGoal {
    const fn new() -> Self {
        Self {
            inner: BreedGoal::new(BREED_SPEED_MOD),
            unhappy_cooldown: 0,
        }
    }

    /// Vanilla parity: `PandaBreedGoal.canFindBamboo`, an eight-block box three
    /// layers deep.
    fn can_find_bamboo(mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        let origin = mob.block_position();
        for y in 0..BAMBOO_SEARCH_HEIGHT {
            for x in -BAMBOO_SEARCH_RADIUS..=BAMBOO_SEARCH_RADIUS {
                for z in -BAMBOO_SEARCH_RADIUS..=BAMBOO_SEARCH_RADIUS {
                    let pos = BlockPos::new(origin.x() + x, origin.y() + y, origin.z() + z);
                    if world.get_block_state(pos).get_block() == &vanilla_blocks::BAMBOO {
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl Goal for PandaBreedGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if !self.inner.can_use(mob) {
            return false;
        }
        let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
            return false;
        };
        if panda.unhappy_counter() != 0 {
            return false;
        }
        if Self::can_find_bamboo(mob) {
            return true;
        }

        if self.unhappy_cooldown <= panda.tick_count() {
            panda.set_unhappy_counter(TOTAL_UNHAPPY_TIME);
            self.unhappy_cooldown = panda.tick_count() + UNHAPPY_COOLDOWN;
            if let Some(world) = panda.level() {
                let position = panda.position();
                let nearest = world
                    .nearest_player(position, BREED_TARGETING_RANGE, |player| {
                        !player.is_spectator()
                    })
                    .map(|player| -> SharedEntity { player });
                panda.set_unhappy_look_target(nearest.as_ref());
            }
        }
        false
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}

/// Vanilla parity: `Panda.PandaHurtByTargetGoal`. A panda that has been fed or
/// has bitten back drops its target, and the alert only reaches the aggressive
/// pandas -- everybody else stays out of it.
struct PandaHurtByTargetGoal {
    inner: HurtByTargetGoal,
}

impl PandaHurtByTargetGoal {
    fn new() -> Self {
        Self {
            inner: HurtByTargetGoal::new()
                .set_alert_others([])
                .with_alert_filter(Mob::is_aggressive),
        }
    }
}

impl Goal for PandaHurtByTargetGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let bought_off =
            with_panda(mob, |panda| panda.got_bamboo() || panda.did_bite()).unwrap_or(false);
        if bought_off {
            mob.set_target(None);
            return false;
        }
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}

/// Vanilla parity: `Panda.PandaLieOnBackGoal`.
struct PandaLieOnBackGoal {
    cooldown: i32,
}

impl PandaLieOnBackGoal {
    const fn new() -> Self {
        Self { cooldown: 0 }
    }
}

impl Goal for PandaLieOnBackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
            return false;
        };
        self.cooldown < panda.tick_count()
            && panda.is_lazy()
            && panda.can_perform_action()
            && rand::random_range(0..reduced_tick_delay(LIE_ON_BACK_CHANCE)) == 1
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        rest_continues(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(panda) = mob.downcast_ref::<PandaEntity>() {
            panda.set_on_back(true);
        }
        self.cooldown = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
            return;
        };
        panda.set_on_back(false);
        self.cooldown = panda.tick_count() + LIE_ON_BACK_COOLDOWN;
    }
}

/// Vanilla parity: the `canContinueToUse` both resting goals share, whose two
/// rolls are what makes a lazy panda stay put far longer than any other.
fn rest_continues(mob: &dyn PathfinderMob) -> bool {
    let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
        return false;
    };
    if panda.is_in_water() {
        return false;
    }
    if !panda.is_lazy() && rand::random_range(0..reduced_tick_delay(REST_INTERRUPT_CHANCE)) == 1 {
        return false;
    }
    rand::random_range(0..reduced_tick_delay(REST_END_CHANCE)) != 1
}

/// Vanilla parity: `Panda.PandaSitGoal`, which is the bamboo-eating loop: walk
/// to a dropped stalk, sit down with it, and drop what is left when bored.
struct PandaSitGoal {
    cooldown: i32,
}

impl PandaSitGoal {
    const fn new() -> Self {
        Self { cooldown: 0 }
    }

    /// The bamboo lying within `range` of the panda.
    fn nearby_bamboo(mob: &dyn PathfinderMob, range: f64) -> Vec<SharedEntity> {
        let Some(world) = mob.level() else {
            return Vec::new();
        };
        world.get_entities_in_aabb_matching(&mob.bounding_box().inflate(range), |entity| {
            PandaEntity::can_pick_up_and_eat(entity)
        })
    }
}

impl Goal for PandaSitGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
            return false;
        };
        if self.cooldown > panda.tick_count()
            || AgeableMob::is_baby(panda)
            || panda.is_in_water()
            || !panda.can_perform_action()
            || panda.unhappy_counter() > 0
        {
            return false;
        }
        if !panda.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
            return true;
        }
        !Self::nearby_bamboo(mob, SIT_ITEM_SEARCH_RANGE).is_empty()
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        rest_continues(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
            return;
        };
        if panda.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
            if let Some(bamboo) = Self::nearby_bamboo(mob, SIT_ITEM_WALK_RANGE).first() {
                let path = panda.create_path_to(bamboo.block_position(), 0);
                panda.move_to_path(path, SIT_WALK_SPEED_MOD);
            }
        } else {
            panda.try_to_sit();
        }
        self.cooldown = 0;
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
            return;
        };
        if !panda.is_sitting() && !panda.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
            panda.try_to_sit();
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
            return;
        };
        let held = panda.get_item_by_slot(EquipmentSlot::MainHand);
        if !held.is_empty() {
            panda.spawn_at_location(held, 0.0);
            panda.set_item_slot(EquipmentSlot::MainHand, ItemStack::empty());
            let bound = if panda.is_lazy() {
                SIT_COOLDOWN_LAZY_BOUND
            } else {
                SIT_COOLDOWN_BOUND
            };
            let wait_seconds = rand::random_range(0..bound) + SIT_COOLDOWN_BASE;
            self.cooldown = panda.tick_count() + wait_seconds * TICKS_PER_SECOND;
        }
        panda.sit(false);
    }
}

/// Vanilla parity: `Panda.PandaSneezeGoal`. Only a cub sneezes, and a weak cub
/// sneezes twelve times as often as any other.
struct PandaSneezeGoal;

impl Goal for PandaSneezeGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
            return false;
        };
        if !AgeableMob::is_baby(panda) || !panda.can_perform_action() {
            return false;
        }
        if panda.is_weak() && rand::random_range(0..reduced_tick_delay(WEAK_SNEEZE_CHANCE)) == 1 {
            return true;
        }
        rand::random_range(0..reduced_tick_delay(SNEEZE_CHANCE)) == 1
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(panda) = mob.downcast_ref::<PandaEntity>() {
            panda.sneeze(true);
        }
    }
}

/// Vanilla parity: `Panda.PandaRollGoal`. A cub or a playful panda rolls, and
/// it rolls at once when the block ahead of it is a drop.
struct PandaRollGoal;

impl Goal for PandaRollGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK | GoalControls::JUMP
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(panda) = mob.downcast_ref::<PandaEntity>() else {
            return false;
        };
        if !(AgeableMob::is_baby(panda) || panda.is_playful()) || !panda.on_ground() {
            return false;
        }
        if !panda.can_perform_action() {
            return false;
        }

        let angle = panda.rotation().0.to_radians();
        let x_dir = -angle.sin();
        let z_dir = angle.cos();
        let step = |dir: f32| {
            if dir.abs() > ROLL_STEP_THRESHOLD {
                dir.signum() as i32
            } else {
                0
            }
        };
        let Some(world) = panda.level() else {
            return false;
        };
        let ahead = panda.block_position();
        let ahead = BlockPos::new(
            ahead.x() + step(x_dir),
            ahead.y() - 1,
            ahead.z() + step(z_dir),
        );
        if world.get_block_state(ahead).is_air() {
            return true;
        }

        if panda.is_playful() && rand::random_range(0..reduced_tick_delay(PLAYFUL_ROLL_CHANCE)) == 1
        {
            return true;
        }
        rand::random_range(0..reduced_tick_delay(ROLL_CHANCE)) == 1
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        false
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(panda) = mob.downcast_ref::<PandaEntity>() {
            panda.roll(true);
        }
    }
}
