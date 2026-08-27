//! The four-crystal ritual that brings the dragon back.
//!
//! Vanilla parity: `DragonRespawnStage`. Vanilla writes this as an enum whose
//! constants each override `tick`; the Rust form is one `match` over the same
//! five states, driven by [`EnderDragonFight::tick`](super::EnderDragonFight).
//!
//! Every position here is the literal `BlockPos(0, 128, 0)` vanilla writes, not
//! an offset from the fight origin. That is deliberate on vanilla's side and
//! kept deliberately: the beams of a respawn always converge on the world
//! origin even when the fight is centered elsewhere.

use std::sync::Arc;

use glam::DVec3;
use serde::{Deserialize, Serialize};
use steel_registry::REGISTRY;
use steel_registry::feature::{ConfiguredFeatureKind, EndSpike, EndSpikeConfiguration};
use steel_registry::level_events;
use steel_registry::vanilla_game_rules::BLOCK_EXPLOSION_DROP_DECAY;
use steel_utils::random::worldgen_random::WorldgenRandom;
use steel_utils::{BlockPos, Downcast as _};

use super::fight::EnderDragonFight;
use crate::entity::entities::EndCrystalEntity;
use crate::entity::{Entity as _, RemovalReason, SharedEntity};
use crate::world::World;
use crate::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};
use crate::worldgen::feature::FeatureDecorationRunner;

/// Where every beam of a respawn points, and where the dragon comes back.
///
/// Vanilla parity: the `new BlockPos(0, 128, 0)` each stage writes out.
pub(super) const BEAM_ORIGIN: BlockPos = BlockPos::new(0, 128, 0);

/// Ticks the pillar stage spends on each spike.
///
/// Vanilla parity: the `int interval = 40` of `SUMMONING_PILLARS`.
const PILLAR_INTERVAL: i32 = 40;

/// How far around a spike the ritual clears before rebuilding it.
///
/// Vanilla parity: the `int radius = 10` of `SUMMONING_PILLARS`.
const PILLAR_CLEAR_RADIUS: i32 = 10;

/// Y the rebuilt spike feature is placed from.
///
/// Vanilla parity: the `new BlockPos(spike.getCenterX(), 45, spike.getCenterZ())`
/// origin `SUMMONING_PILLARS` hands the feature.
const PILLAR_PLACE_Y: i32 = 45;

/// Blast that clears the old pillar away.
///
/// Vanilla parity: the `5.0F` of `SUMMONING_PILLARS`.
const PILLAR_EXPLOSION_RADIUS: f32 = 5.0;

/// Blast each ritual crystal leaves when the dragon arrives.
///
/// Vanilla parity: the `6.0F` of `SUMMONING_DRAGON`.
const RITUAL_CRYSTAL_EXPLOSION_RADIUS: f32 = 6.0;

/// How long the crystals charge before the pillars start.
///
/// Vanilla parity: the `time < 100` of `PREPARING_TO_SUMMON_PILLARS`.
const PREPARING_TICKS: i32 = 100;

/// How long the beams hold on the origin before the dragon arrives.
///
/// Vanilla parity: the `time >= 100` of `SUMMONING_DRAGON`.
const SUMMONING_TICKS: i32 = 100;

/// Tick the summoning roar starts repeating on.
///
/// Vanilla parity: the `time >= 80` of `SUMMONING_DRAGON`.
const SUMMONING_ROAR_START_TICK: i32 = 80;

/// How long the opening roar of the summon runs for.
///
/// Vanilla parity: the `time < 5` of `SUMMONING_DRAGON`.
const SUMMONING_OPENING_ROAR_TICKS: i32 = 5;

/// The stage a running respawn ritual is in.
///
/// Vanilla parity: `DragonRespawnStage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DragonRespawnStage {
    /// The crystals lock onto the origin.
    Start,
    /// The crystals charge.
    PreparingToSummonPillars,
    /// One pillar is rebuilt every two seconds.
    SummoningPillars,
    /// The beams hold on the origin and the dragon arrives.
    SummoningDragon,
    /// Nothing left to do.
    End,
}

impl DragonRespawnStage {
    /// Runs one tick of this stage.
    ///
    /// Vanilla parity: `DragonRespawnStage.tick`. `time` is the fight's
    /// `respawnTime` *before* this tick's increment, which is what makes the
    /// `time == 0` branches fire on the first tick of a stage.
    pub(super) fn tick(
        self,
        world: &Arc<World>,
        fight: &EnderDragonFight,
        crystals: &[SharedEntity],
        time: i32,
    ) {
        match self {
            Self::Start => {
                for crystal in end_crystals(crystals) {
                    crystal.set_beam_target(Some(BEAM_ORIGIN));
                }
                fight.set_respawn_stage(world, Self::PreparingToSummonPillars);
            }
            Self::PreparingToSummonPillars => {
                if time >= PREPARING_TICKS {
                    fight.set_respawn_stage(world, Self::SummoningPillars);
                    return;
                }
                if time == 0 || (50..=52).contains(&time) || time >= PREPARING_TICKS - 5 {
                    world.level_event(
                        level_events::ANIMATION_DRAGON_SUMMON_ROAR,
                        BEAM_ORIGIN,
                        0,
                        None,
                    );
                }
            }
            Self::SummoningPillars => Self::tick_summoning_pillars(world, fight, crystals, time),
            Self::SummoningDragon => Self::tick_summoning_dragon(world, fight, crystals, time),
            Self::End => {}
        }
    }

    /// Vanilla parity: the `SUMMONING_PILLARS` body.
    fn tick_summoning_pillars(
        world: &Arc<World>,
        fight: &EnderDragonFight,
        crystals: &[SharedEntity],
        time: i32,
    ) {
        let start_of_beam = time % PILLAR_INTERVAL == 0;
        let end_of_beam = time % PILLAR_INTERVAL == PILLAR_INTERVAL - 1;
        if !start_of_beam && !end_of_beam {
            return;
        }

        let spikes = FeatureDecorationRunner::end_spikes_for_level(world.seed());
        let index = (time / PILLAR_INTERVAL) as usize;
        let Some(spike) = spikes.get(index) else {
            if start_of_beam {
                fight.set_respawn_stage(world, Self::SummoningDragon);
            }
            return;
        };

        if start_of_beam {
            let target = BlockPos::new(spike.center_x, spike.height + 1, spike.center_z);
            for crystal in end_crystals(crystals) {
                crystal.set_beam_target(Some(target));
            }
            return;
        }

        Self::rebuild_spike(world, spike);
    }

    /// Clears the old pillar away and puts a fresh, caged one back.
    ///
    /// Vanilla parity: the `endOfBeam` branch of `SUMMONING_PILLARS`.
    fn rebuild_spike(world: &Arc<World>, spike: &EndSpike) {
        let low = BlockPos::new(
            spike.center_x - PILLAR_CLEAR_RADIUS,
            spike.height - PILLAR_CLEAR_RADIUS,
            spike.center_z - PILLAR_CLEAR_RADIUS,
        );
        let high = BlockPos::new(
            spike.center_x + PILLAR_CLEAR_RADIUS,
            spike.height + PILLAR_CLEAR_RADIUS,
            spike.center_z + PILLAR_CLEAR_RADIUS,
        );
        for pos in BlockPos::between_closed(low, high) {
            world.remove_block(pos, false);
        }

        world.explode(
            ExplosionSpec::new(
                None,
                None,
                None,
                PILLAR_EXPLOSION_RADIUS,
                false,
                world.explosion_destroy_type(&BLOCK_EXPLOSION_DROP_DECAY),
            ),
            DVec3::new(
                f64::from(spike.center_x) + 0.5,
                f64::from(spike.height),
                f64::from(spike.center_z) + 0.5,
            ),
        );

        // Vanilla parity: the respawned pillars carry invulnerable crystals
        // whose beams still point at the origin, which is what keeps the ritual
        // readable while the last pillars go up.
        let configuration = EndSpikeConfiguration {
            spikes: vec![spike.clone()],
            crystal_invulnerable: true,
            crystal_beam_target: Some(BEAM_ORIGIN.0),
        };
        let mut random = WorldgenRandom::from_seed(rand::random());
        FeatureDecorationRunner::place_configured_feature_kind(
            world,
            &REGISTRY,
            &mut random,
            &ConfiguredFeatureKind::EndSpike(configuration),
            BlockPos::new(spike.center_x, PILLAR_PLACE_Y, spike.center_z),
            world.biome_zoom_seed(),
        );
    }

    /// Vanilla parity: the `SUMMONING_DRAGON` body.
    fn tick_summoning_dragon(
        world: &Arc<World>,
        fight: &EnderDragonFight,
        crystals: &[SharedEntity],
        time: i32,
    ) {
        if time >= SUMMONING_TICKS {
            fight.set_respawn_stage(world, Self::End);
            EnderDragonFight::reset_spike_crystals(world);
            for crystal in end_crystals(crystals) {
                crystal.set_beam_target(None);
                world.explode(
                    ExplosionSpec::new(
                        Some(crystal.id()),
                        None,
                        None,
                        RITUAL_CRYSTAL_EXPLOSION_RADIUS,
                        false,
                        ExplosionBlockInteraction::Keep,
                    ),
                    crystal.position(),
                );
                crystal.set_removed(RemovalReason::Discarded);
            }
            return;
        }

        if time >= SUMMONING_ROAR_START_TICK {
            world.level_event(
                level_events::ANIMATION_DRAGON_SUMMON_ROAR,
                BEAM_ORIGIN,
                0,
                None,
            );
        } else if time == 0 {
            for crystal in end_crystals(crystals) {
                crystal.set_beam_target(Some(BEAM_ORIGIN));
            }
        } else if time < SUMMONING_OPENING_ROAR_TICKS {
            world.level_event(
                level_events::ANIMATION_DRAGON_SUMMON_ROAR,
                BEAM_ORIGIN,
                0,
                None,
            );
        }
    }
}

/// Reads the ritual crystals back out of the erased entities the fight resolved.
///
/// The fight looks the crystals up by UUID and can only hand back
/// [`SharedEntity`]; Steel has no downcasting for `Arc`, so each stage narrows
/// them again where it uses them.
fn end_crystals(entities: &[SharedEntity]) -> impl Iterator<Item = &EndCrystalEntity> {
    entities
        .iter()
        .filter_map(|entity| entity.downcast_ref::<EndCrystalEntity>())
}
