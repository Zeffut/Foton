//! The tempt goal an ocelot follows fish with.
//!
//! Vanilla parity: `Ocelot.OcelotTemptGoal`, whose only change to `TemptGoal`
//! is that a trusting ocelot can no longer be scared off mid-follow.

use std::sync::Arc;

use foton_utils::Downcast as _;

use super::OcelotEntity;
use crate::entity::PathfinderMob;
use crate::entity::ai::goal::{TemptGoal, TemptScareRule};
use crate::player::Player;

struct OcelotTemptScareRule;

impl TemptScareRule for OcelotTemptScareRule {
    fn can_scare(
        &mut self,
        mob: &dyn PathfinderMob,
        _player: Option<&Arc<Player>>,
        base: bool,
    ) -> bool {
        let trusting = mob
            .downcast_ref::<OcelotEntity>()
            .is_some_and(OcelotEntity::is_trusting);
        base && !trusting
    }
}

/// Builds vanilla `Ocelot.OcelotTemptGoal`.
#[must_use]
pub(super) fn new(speed_modifier: f64) -> TemptGoal {
    TemptGoal::new(speed_modifier, OcelotEntity::is_ocelot_food, true)
        .with_scare_rule(OcelotTemptScareRule)
}
