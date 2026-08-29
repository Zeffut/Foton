use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_damage_type_tags;
use foton_utils::{BlockPos, Identifier};
use glam::DVec3;

use super::random_pos::default_random_pos;
use super::selector::{Goal, GoalControls};
use crate::behavior::{BLOCK_BEHAVIORS, BlockCollisionContext};
use crate::entity::PathfinderMob;
use crate::fluid::FluidStateExt as _;

const WATER_CHECK_DISTANCE_VERTICAL: i32 = 1;

/// Which damage makes this mob run.
///
/// Vanilla parity: the `Function<PathfinderMob, TagKey<DamageType>>` of the
/// third `PanicGoal` constructor. A polar bear cub panics at anything; a grown
/// bear only at the environment, because it fights back against everything else.
type PanicCausingDamageTypes = Box<dyn Fn(&dyn PathfinderMob) -> Identifier + Send + Sync>;

/// An extra condition a subclass puts in front of `PanicGoal.shouldPanic`.
///
/// Vanilla parity: the `shouldPanic` overrides that read `... && super.shouldPanic()`,
/// which is why this narrows the goal rather than replacing its damage test.
type PanicFilter = Box<dyn Fn(&dyn PathfinderMob) -> bool + Send>;

pub struct PanicGoal {
    wanted_position: Option<DVec3>,
    speed_modifier: f64,
    is_running: bool,
    panic_causing_damage_types: Option<PanicCausingDamageTypes>,
    panic_filter: Option<PanicFilter>,
}

impl PanicGoal {
    #[must_use]
    pub(crate) const fn new(speed_modifier: f64) -> Self {
        // No stored tag: `should_panic` falls back to `PANIC_CAUSES`, which is
        // what this constructor would otherwise have to box a closure to say.
        Self {
            wanted_position: None,
            speed_modifier,
            is_running: false,
            panic_causing_damage_types: None,
            panic_filter: None,
        }
    }

    /// Creates a panic goal that only the given damage types set off.
    ///
    /// Vanilla parity: the `PanicGoal(mob, speedModifier, panicCausingDamageTypes)`
    /// constructor, which is how a wolf panics at drowning but not at a punch.
    #[must_use]
    pub(crate) fn with_damage_types(
        speed_modifier: f64,
        panic_causing_damage_types: Identifier,
    ) -> Self {
        Self::with_panic_causing_damage_types(speed_modifier, move |_| {
            panic_causing_damage_types.clone()
        })
    }

    /// Creates a panic goal that runs from something other than the usual.
    ///
    /// Vanilla parity: the `TagKey<DamageType>` and `Function` constructors.
    #[must_use]
    pub(crate) fn with_panic_causing_damage_types(
        speed_modifier: f64,
        panic_causing_damage_types: impl Fn(&dyn PathfinderMob) -> Identifier + Send + Sync + 'static,
    ) -> Self {
        Self {
            wanted_position: None,
            speed_modifier,
            is_running: false,
            panic_causing_damage_types: Some(Box::new(panic_causing_damage_types)),
            panic_filter: None,
        }
    }

    /// Adds a condition that must also hold before the mob panics.
    ///
    /// Vanilla parity: a `shouldPanic` override that narrows rather than
    /// replaces the base test, as `Fox.FoxPanicGoal` does with `!isDefending()`.
    #[must_use]
    pub(crate) fn with_panic_filter(
        mut self,
        filter: impl Fn(&dyn PathfinderMob) -> bool + Send + 'static,
    ) -> Self {
        self.panic_filter = Some(Box::new(filter));
        self
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.is_running
    }

    /// Vanilla parity: the protected `PanicGoal.shouldPanic`, and the
    /// subclass overrides that narrow it before deferring to it.
    pub(crate) fn should_panic(&self, mob: &dyn PathfinderMob) -> bool {
        if let Some(filter) = &self.panic_filter
            && !filter(mob)
        {
            return false;
        }

        let tag = match &self.panic_causing_damage_types {
            Some(damage_types) => damage_types(mob),
            None => vanilla_damage_type_tags::DamageTypeTag::PANIC_CAUSES,
        };
        mob.last_damage_source()
            .is_some_and(|source| source.is(&tag))
    }

    /// Vanilla parity: writing the protected `posX`/`posY`/`posZ` fields, as
    /// `Turtle.TurtlePanicGoal.canUse` does after finding water.
    pub(crate) const fn set_wanted_position(&mut self, wanted_position: DVec3) {
        self.wanted_position = Some(wanted_position);
    }

    /// Vanilla parity: the protected `PanicGoal.findRandomPosition`.
    pub(crate) fn find_random_position(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(position) = default_random_pos(mob, 5, 4) else {
            return false;
        };

        self.wanted_position = Some(position);
        true
    }
}

impl Goal for PanicGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn is_panic_goal(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if !self.should_panic(mob) {
            return false;
        }

        if mob.is_on_fire()
            && let Some(water_pos) = look_for_water(mob, 5)
        {
            self.wanted_position = Some(block_pos_corner(water_pos));
            return true;
        }

        self.find_random_position(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !mob.mob_base().navigation().lock().is_done()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(wanted_position) = self.wanted_position {
            mob.move_to_pos(wanted_position, self.speed_modifier);
        }
        self.is_running = true;
    }

    fn stop(&mut self, _mob: &dyn PathfinderMob) {
        self.is_running = false;
    }
}

pub(crate) fn look_for_water(mob: &dyn PathfinderMob, xz_dist: i32) -> Option<BlockPos> {
    let world = mob.level()?;
    let mob_position = mob.block_position();
    let block_state = world.get_block_state(mob_position);
    let behavior = BLOCK_BEHAVIORS.get_behavior(block_state.get_block());
    if !behavior
        .get_collision_shape(
            block_state,
            world.as_ref(),
            mob_position,
            BlockCollisionContext::empty(),
        )
        .is_empty()
    {
        return None;
    }

    mob_position.find_closest_match(xz_dist, WATER_CHECK_DISTANCE_VERTICAL, |pos| {
        world.get_block_state(pos).get_fluid_state().is_water()
    })
}

pub(crate) fn block_pos_corner(pos: BlockPos) -> DVec3 {
    DVec3::new(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()))
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use foton_registry::{init_vanilla_registry, vanilla_damage_types, vanilla_entities};

    use super::*;
    use crate::entity::LivingEntity;
    use crate::entity::damage::DamageSource;
    use crate::entity::entities::PigEntity;
    use crate::test_support::test_world;

    #[test]
    fn panic_goal_uses_move_control() {
        let goal = PanicGoal::new(1.25);

        assert_eq!(goal.controls(), GoalControls::MOVE);
        assert!(!goal.is_running());
    }

    #[test]
    fn panic_goal_uses_vanilla_panic_damage_tag() {
        init_vanilla_registry();
        let pig = PigEntity::new(&vanilla_entities::PIG, 1, DVec3::ZERO, Weak::new());
        let goal = PanicGoal::new(1.25);

        assert!(!goal.should_panic(&pig));

        assert!(pig.hurt_server(
            test_world(),
            &DamageSource::environment(&vanilla_damage_types::GENERIC),
            1.0
        ));
        assert!(!goal.should_panic(&pig));

        assert!(pig.hurt_server(
            test_world(),
            &DamageSource::environment(&vanilla_damage_types::PLAYER_ATTACK),
            2.0
        ));
        assert!(goal.should_panic(&pig));
    }

    /// Vanilla parity: the `PanicGoal(mob, speed, PANIC_ENVIRONMENTAL_CAUSES)`
    /// a wolf registers, which is why hitting a tamed wolf makes it fight back
    /// rather than bolt.
    #[test]
    fn a_narrowed_panic_goal_ignores_damage_outside_its_tag() {
        init_vanilla_registry();
        let pig = PigEntity::new(&vanilla_entities::PIG, 1, DVec3::ZERO, Weak::new());
        let goal = PanicGoal::with_damage_types(
            1.0,
            vanilla_damage_type_tags::DamageTypeTag::PANIC_ENVIRONMENTAL_CAUSES,
        );

        assert!(pig.hurt_server(
            test_world(),
            &DamageSource::environment(&vanilla_damage_types::PLAYER_ATTACK),
            2.0
        ));

        assert!(!goal.should_panic(&pig));
    }
}
