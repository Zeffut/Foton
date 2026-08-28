//! Explosions.
//!
//! Vanilla parity: `ServerExplosion`. An explosion casts rays out of its center,
//! spends power against the blocks it crosses, damages and pushes nearby
//! entities by how much of them the blast can see, and optionally leaves fire.

use std::sync::Arc;

use glam::DVec3;
use steel_protocol::packets::game::CExplode;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::game_rules::GameRule;
use steel_registry::particle_type::{ExplosionParticleInfo, ParticleData};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::{
    sound_events, vanilla_attributes, vanilla_blocks, vanilla_damage_types, vanilla_particle_types,
};
use steel_utils::random::weighted_list::{Weighted, WeightedList};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, WorldAabb};

use crate::entity::Entity;
use crate::entity::damage::DamageSource;
use crate::fluid::get_fluid_state;
use crate::world::World;
use crate::world::raycast::{ClipBlockShape, ClipFluid};

/// How far a player can be from a blast and still be told about it.
///
/// Vanilla parity: the `distanceToSqr(center) < 4096.0` of `ServerLevel.explode`.
const EXPLOSION_PACKET_RANGE_SQ: f64 = 4096.0;

/// What an explosion does to the blocks it reaches.
///
/// Vanilla parity: `Explosion.BlockInteraction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplosionBlockInteraction {
    /// Leaves every block standing.
    Keep,
    /// Breaks blocks and drops all of their loot.
    Destroy,
    /// Breaks blocks and thins their loot by the blast radius.
    ///
    /// The only thing this changes is that the block loot roll receives the
    /// explosion radius, which is what `survives_explosion` and
    /// `explosion_decay` read. Which of the two destroying modes a blast uses
    /// is decided by the matching `*_explosion_drop_decay` game rule -- see
    /// [`World::explosion_destroy_type`].
    DestroyWithDecay,
}

impl ExplosionBlockInteraction {
    /// Whether this interaction breaks the blocks the blast reaches.
    #[must_use]
    pub const fn destroys_blocks(self) -> bool {
        matches!(self, Self::Destroy | Self::DestroyWithDecay)
    }

    /// The blast radius handed to the block loot roll, if drops decay.
    #[must_use]
    const fn loot_explosion_radius(self, radius: f32) -> Option<f32> {
        match self {
            Self::DestroyWithDecay => Some(radius),
            Self::Keep | Self::Destroy => None,
        }
    }
}

/// Rays are cast from a 16x16x16 grid, using only its outer shell.
const RAY_GRID_SIZE: i32 = 16;
/// Distance each ray advances per step.
const RAY_STEP: f32 = 0.3;
/// Power every ray loses per step regardless of what it crosses.
const RAY_STEP_COST: f32 = 0.225_000_01;

/// An explosion, apart from where it lands.
///
/// Both [`World::explode`] and [`World::explode_sparing`] take one. It used to
/// be the sparing variant's alone, with the plain one keeping its arguments
/// loose; splitting the source entity into vanilla's direct/causing pair
/// pushed that list past what one call can carry legibly.
pub struct ExplosionSpec {
    /// The entity the blast came out of, if any: the TNT, the fireball, the
    /// creeper.
    ///
    /// Vanilla parity: `Explosion.getDirectSourceEntity`.
    pub direct_entity_id: Option<i32>,
    /// Who is ultimately to blame, if anyone: the player who lit the TNT, the
    /// ghast that spat the fireball, or the creeper itself.
    ///
    /// Vanilla parity: `Explosion.getIndirectSourceEntity`, which is a switch
    /// over the source's class. Steel has no class hierarchy to switch on, so
    /// each caller passes its own answer -- a primed TNT its owner, a
    /// projectile its living shooter, a living entity itself.
    pub causing_entity_id: Option<i32>,
    /// What the blast hurts entities with.
    pub damage_source: Option<DamageSource>,
    /// How far it reaches.
    pub radius: f32,
    /// Whether it leaves fires behind.
    pub fire: bool,
    /// What it does to blocks.
    pub interaction: ExplosionBlockInteraction,
    /// Whether the blast hurts what it reaches.
    ///
    /// Vanilla parity: the `damagesEntities` of
    /// `SimpleExplosionDamageCalculator`. A wind charge is the reason this
    /// exists: it shoves everything around it and hurts none of it.
    pub damages_entities: bool,
    /// How hard the blast shoves, against an ordinary explosion.
    ///
    /// Vanilla parity: the `knockbackMultiplier` of the same calculator.
    pub knockback_multiplier: f64,
    /// The emitter drawn at the center of a blast the client calls small.
    ///
    /// Vanilla parity: the `smallExplosionParticles` argument of
    /// `Level.explode`.
    pub small_particle: ParticleData,
    /// The emitter drawn at the center of every other blast.
    ///
    /// Vanilla parity: the `largeExplosionParticles` argument.
    pub large_particle: ParticleData,
    /// The debris the broken blocks throw off.
    ///
    /// Vanilla parity: the `blockParticles` argument.
    pub block_particles: WeightedList<ExplosionParticleInfo>,
    /// The sound the blast makes.
    ///
    /// Vanilla parity: the `explosionSound` argument. It travels inside the
    /// explosion packet rather than an ordinary sound packet, so it is the
    /// packet or nothing.
    pub sound: SoundEventRef,
}

/// The debris every blast but a wind charge throws off.
///
/// Vanilla parity: `Level.DEFAULT_EXPLOSION_BLOCK_PARTICLES`, whose builder
/// gives each entry the default weight of one.
#[must_use]
fn default_explosion_block_particles() -> WeightedList<ExplosionParticleInfo> {
    WeightedList::new(vec![
        Weighted {
            value: ExplosionParticleInfo::new(
                ParticleData::simple(&vanilla_particle_types::POOF),
                0.5,
                1.0,
            ),
            weight: 1,
        },
        Weighted {
            value: ExplosionParticleInfo::new(
                ParticleData::simple(&vanilla_particle_types::SMOKE),
                1.0,
                1.0,
            ),
            weight: 1,
        },
    ])
}

impl ExplosionSpec {
    /// An ordinary blast: it hurts, and it shoves as hard as TNT.
    ///
    /// Vanilla parity: the four `Level.explode` overloads that fill in the
    /// presentation themselves, which is every caller but the two wind charges.
    #[must_use]
    pub fn new(
        direct_entity_id: Option<i32>,
        causing_entity_id: Option<i32>,
        damage_source: Option<DamageSource>,
        radius: f32,
        fire: bool,
        interaction: ExplosionBlockInteraction,
    ) -> Self {
        Self {
            direct_entity_id,
            causing_entity_id,
            damage_source,
            radius,
            fire,
            interaction,
            damages_entities: true,
            knockback_multiplier: 1.0,
            small_particle: ParticleData::simple(&vanilla_particle_types::EXPLOSION),
            large_particle: ParticleData::simple(&vanilla_particle_types::EXPLOSION_EMITTER),
            block_particles: default_explosion_block_particles(),
            sound: &sound_events::ENTITY_GENERIC_EXPLODE,
        }
    }

    /// Whether the client draws the small emitter rather than the large one.
    ///
    /// Vanilla parity: `ServerExplosion.isSmall`.
    #[must_use]
    fn is_small(&self) -> bool {
        self.radius < 2.0 || !self.interaction.destroys_blocks()
    }

    /// The emitter this blast draws at its center.
    #[must_use]
    fn explosion_particle(&self) -> ParticleData {
        if self.is_small() {
            self.small_particle.clone()
        } else {
            self.large_particle.clone()
        }
    }
}

/// The damage a blast deals when its caller does not name one.
///
/// Vanilla parity: `DamageSources.explosion(entity, cause)`, which picks
/// `player_explosion` when the blast is attributed to someone and `explosion`
/// when it is not. The two are not interchangeable: only `player_explosion` is
/// in `can_break_armor_stand`, and it carries the `explosion.player` death
/// message. Despite the name it is not about players -- vanilla's
/// `getIndirectSourceEntity` returns a living source unchanged, so a creeper is
/// its own cause and its blast is a `player_explosion` too. A TNT nobody lit is
/// the ordinary kind.
#[must_use]
fn default_explosion_damage_source(
    direct_entity_id: Option<i32>,
    causing_entity_id: Option<i32>,
) -> DamageSource {
    DamageSource::environment(
        if direct_entity_id.is_some() && causing_entity_id.is_some() {
            &vanilla_damage_types::PLAYER_EXPLOSION
        } else {
            &vanilla_damage_types::EXPLOSION
        },
    )
}

impl World {
    /// Detonates an explosion and returns the positions it destroyed.
    ///
    /// Vanilla parity: `ServerExplosion.explode`.
    pub fn explode(self: &Arc<Self>, spec: ExplosionSpec, center: DVec3) -> Vec<BlockPos> {
        self.explode_sparing(spec, center, &|_pos| true)
    }

    /// Picks between plain destruction and destruction that decays drops.
    ///
    /// Vanilla parity: `ServerLevel.getDestroyType`. Each explosion source has
    /// its own `*_explosion_drop_decay` game rule.
    #[must_use]
    pub fn explosion_destroy_type(&self, decay_rule: &GameRule<bool>) -> ExplosionBlockInteraction {
        if self.get_game_rule(decay_rule) {
            ExplosionBlockInteraction::DestroyWithDecay
        } else {
            ExplosionBlockInteraction::Destroy
        }
    }

    /// Detonates an explosion that leaves some blocks standing.
    ///
    /// Vanilla parity: the `Explosion.shouldBlockExplode` hook, which the
    /// source entity answers. Only the TNT minecart uses it, and only to spare
    /// the rails it was running on -- otherwise one cart would take the track
    /// with it and no cart cannon would work twice.
    pub fn explode_sparing(
        self: &Arc<Self>,
        spec: ExplosionSpec,
        center: DVec3,
        should_explode: &dyn Fn(BlockPos) -> bool,
    ) -> Vec<BlockPos> {
        let mut to_blow = self.calculate_exploded_positions(center, spec.radius);
        to_blow.retain(|pos| should_explode(*pos));
        let hit_players = self.hurt_entities_from_explosion(&spec, center);

        if spec.interaction.destroys_blocks() {
            let loot_radius = spec.interaction.loot_explosion_radius(spec.radius);
            // `ServerExplosion.source` is the direct entity, not the one to
            // blame: a fireball's drops are the fireball's context, even
            // though the ghast is what answers for the damage.
            let source_entity = spec
                .direct_entity_id
                .and_then(|entity_id| self.get_entity_by_id(entity_id));
            for pos in &to_blow {
                self.destroy_block_from_explosion(
                    *pos,
                    source_entity.as_deref(),
                    loot_radius,
                    spec.causing_entity_id,
                );
            }
        }

        if spec.fire {
            self.create_explosion_fire(&to_blow);
        }

        self.send_explosion_packets(&spec, center, to_blow.len(), &hit_players);

        to_blow
    }

    /// Tells every player near the blast that it happened, and each pushed one
    /// how hard it pushed them.
    ///
    /// Vanilla parity: the loop at the end of `ServerLevel.explode`. This is the
    /// only channel by which an explosion reaches a client at all -- its sound,
    /// its emitter and its debris all ride here -- and the only one by which it
    /// moves a player. A player is authoritative over their own position, so
    /// the server's `set_velocity` above is a value the player never feels;
    /// `ClientPacketListener.handleExplosion` applying `playerKnockback` to the
    /// local player is what actually launches them.
    fn send_explosion_packets(
        &self,
        spec: &ExplosionSpec,
        center: DVec3,
        block_count: usize,
        hit_players: &[(i32, DVec3)],
    ) {
        let block_count = i32::try_from(block_count).unwrap_or(i32::MAX);
        let particle = spec.explosion_particle();
        self.players.iter_players(|_, player| {
            if player.position().distance_squared(center) >= EXPLOSION_PACKET_RANGE_SQ {
                return true;
            }
            let knockback = hit_players
                .iter()
                .find(|(entity_id, _)| *entity_id == player.id())
                .map(|(_, knockback)| *knockback);
            player.send_packet(CExplode::new(
                center,
                spec.radius,
                block_count,
                knockback,
                particle.clone(),
                spec.sound,
                spec.block_particles.clone(),
            ));
            true
        });
    }

    /// Casts the rays that decide which blocks the blast breaks.
    ///
    /// Vanilla parity: `ServerExplosion.calculateExplodedPositions`.
    fn calculate_exploded_positions(self: &Arc<Self>, center: DVec3, radius: f32) -> Vec<BlockPos> {
        let mut to_blow = Vec::new();
        let last = RAY_GRID_SIZE - 1;

        for x in 0..RAY_GRID_SIZE {
            for y in 0..RAY_GRID_SIZE {
                for z in 0..RAY_GRID_SIZE {
                    // Only the shell of the grid produces rays; the interior would
                    // duplicate directions already covered.
                    if x != 0 && x != last && y != 0 && y != last && z != 0 && z != last {
                        continue;
                    }

                    let mut dx = f64::from(x) / f64::from(last) * 2.0 - 1.0;
                    let mut dy = f64::from(y) / f64::from(last) * 2.0 - 1.0;
                    let mut dz = f64::from(z) / f64::from(last) * 2.0 - 1.0;
                    let length = dx.mul_add(dx, dy.mul_add(dy, dz * dz)).sqrt();
                    if length == 0.0 {
                        continue;
                    }
                    dx /= length;
                    dy /= length;
                    dz /= length;

                    // Vanilla varies each ray's power between 70% and 130%.
                    let mut power = radius * 0.6f32.mul_add(rand::random::<f32>(), 0.7);
                    let mut position = center;

                    while power > 0.0 {
                        let pos = BlockPos::new(
                            position.x.floor() as i32,
                            position.y.floor() as i32,
                            position.z.floor() as i32,
                        );
                        if self.is_outside_build_height(pos.y()) {
                            break;
                        }

                        let state = self.get_block_state(pos);
                        let resistance = self.explosion_resistance_at(pos, state);
                        if let Some(resistance) = resistance {
                            power -= (resistance + 0.3) * RAY_STEP;
                        }

                        if power > 0.0 && !to_blow.contains(&pos) {
                            to_blow.push(pos);
                        }

                        position.x += dx * f64::from(RAY_STEP);
                        position.y += dy * f64::from(RAY_STEP);
                        position.z += dz * f64::from(RAY_STEP);
                        power -= RAY_STEP_COST;
                    }
                }
            }
        }

        to_blow
    }

    /// Returns the resistance a block and its fluid oppose to a blast.
    ///
    /// Vanilla parity: `ExplosionDamageCalculator.getBlockExplosionResistance`.
    /// Air with no fluid offers nothing and costs the ray only its step price.
    fn explosion_resistance_at(
        self: &Arc<Self>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Option<f32> {
        let fluid = get_fluid_state(self, pos);
        if state.is_air() && fluid.is_empty() {
            return None;
        }
        let block_resistance = state.get_block().config.explosion_resistance;
        let fluid_resistance = fluid.fluid_id.explosion_resistance;
        Some(block_resistance.max(fluid_resistance))
    }

    /// Damages and pushes every entity the blast reaches, and returns the push
    /// each player took.
    ///
    /// Vanilla parity: `ServerExplosion.hurtEntities`, whose `hitPlayers` map
    /// exists for exactly one reason: a player's own client has to be handed
    /// the push, because the server's own copy of their velocity is not
    /// something they can feel.
    fn hurt_entities_from_explosion(
        self: &Arc<Self>,
        spec: &ExplosionSpec,
        center: DVec3,
    ) -> Vec<(i32, DVec3)> {
        let mut hit_players = Vec::new();
        let (direct_entity_id, causing_entity_id) = (spec.direct_entity_id, spec.causing_entity_id);
        let radius = spec.radius;
        if radius < 1.0e-5 {
            return hit_players;
        }
        let reach = f64::from(radius) * 2.0;
        let aabb = WorldAabb::new(
            center.x - reach - 1.0,
            center.y - reach - 1.0,
            center.z - reach - 1.0,
            center.x + reach + 1.0,
            center.y + reach + 1.0,
            center.z + reach + 1.0,
        );

        let mut damage_source = spec
            .damage_source
            .clone()
            .unwrap_or_else(|| default_explosion_damage_source(direct_entity_id, causing_entity_id))
            .with_source_position(center);
        if let Some(id) = direct_entity_id {
            damage_source = damage_source.with_direct_entity(id);
        }
        if let Some(id) = causing_entity_id {
            damage_source = damage_source.with_causing_entity(id);
        }

        for entity in self.get_entities_in_aabb(&aabb) {
            // Vanilla parity: `hurtEntities` asks the level for everything
            // *except* the source, so a blast never hits what it came out of.
            if Some(entity.id()) == direct_entity_id {
                continue;
            }
            let distance = entity.position().distance(center) / reach;
            if distance > 1.0 {
                continue;
            }

            let exposure = f64::from(self.seen_percent(center, entity.as_ref()));

            // Vanilla parity: `ExplosionDamageCalculator.getEntityDamageAmount`.
            let impact = (1.0 - distance) * exposure;
            if spec.damages_entities {
                let damage = (impact.mul_add(impact, impact) / 2.0).mul_add(7.0 * reach, 1.0);
                entity.hurt(self, &damage_source, damage as f32);
            }

            // Vanilla pushes from the blast toward the entity's eyes.
            let origin = entity.position().with_y(entity.get_eye_y());
            let direction = origin - center;
            if direction.length_squared() > 0.0 {
                let resistance = entity.as_living_entity().map_or(0.0, |living| {
                    living
                        .attributes()
                        .lock()
                        .required_value(vanilla_attributes::EXPLOSION_KNOCKBACK_RESISTANCE)
                });
                let power = impact * (1.0 - resistance) * spec.knockback_multiplier;
                let knockback = direction.normalize() * power;
                entity.set_velocity(entity.velocity() + knockback);
                entity.mark_velocity_sync();

                // Vanilla parity: the `hitPlayers.put` branch. A spectator is
                // not pushed at all, and neither is a player flying in
                // creative -- their client would fight the shove.
                if let Some(player) = entity.as_player()
                    && !player.is_spectator()
                    && !(player.has_infinite_materials() && player.is_flying())
                {
                    hit_players.push((player.id(), knockback));
                }
            }
        }

        hit_players
    }

    /// Returns the fraction of an entity the blast can see, from 0 to 1.
    ///
    /// Vanilla parity: `ServerExplosion.getSeenPercent`. Vanilla samples a grid of
    /// points across the entity's bounding box and counts how many reach the center
    /// without crossing a block.
    fn seen_percent(&self, center: DVec3, entity: &dyn Entity) -> f32 {
        let bb = entity.bounding_box();
        let step_x = 1.0 / ((bb.max_x() - bb.min_x()) * 2.0 + 1.0);
        let step_y = 1.0 / ((bb.max_y() - bb.min_y()) * 2.0 + 1.0);
        let step_z = 1.0 / ((bb.max_z() - bb.min_z()) * 2.0 + 1.0);
        if step_x < 0.0 || step_y < 0.0 || step_z < 0.0 {
            return 0.0;
        }

        let offset_x = (1.0 - (1.0 / step_x).floor() * step_x) / 2.0;
        let offset_z = (1.0 - (1.0 / step_z).floor() * step_z) / 2.0;

        let mut hits = 0u32;
        let mut total = 0u32;

        let mut fx = 0.0;
        while fx <= 1.0 {
            let mut fy = 0.0;
            while fy <= 1.0 {
                let mut fz = 0.0;
                while fz <= 1.0 {
                    let from = DVec3::new(
                        bb.min_x() + (bb.max_x() - bb.min_x()) * fx + offset_x,
                        bb.min_y() + (bb.max_y() - bb.min_y()) * fy,
                        bb.min_z() + (bb.max_z() - bb.min_z()) * fz + offset_z,
                    );
                    let hit = self.clip(from, center, ClipBlockShape::Collider, ClipFluid::None);
                    if hit.is_miss() {
                        hits += 1;
                    }
                    total += 1;
                    fz += step_z;
                }
                fy += step_y;
            }
            fx += step_x;
        }

        if total == 0 {
            return 0.0;
        }
        hits as f32 / total as f32
    }

    /// Sets fire in a third of the exposed positions above solid ground.
    ///
    /// Vanilla parity: `ServerExplosion.createFire`.
    fn create_explosion_fire(self: &Arc<Self>, positions: &[BlockPos]) {
        for pos in positions {
            if rand::random_range(0..3) != 0 {
                continue;
            }
            if !self.get_block_state(*pos).is_air() {
                continue;
            }
            if !self.get_block_state(pos.below()).is_solid() {
                continue;
            }
            self.set_block(
                *pos,
                vanilla_blocks::FIRE.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::default_explosion_damage_source;

    /// A blast somebody is answerable for is a different damage type from one
    /// nobody is, and the difference is load-bearing: `player_explosion` is in
    /// `can_break_armor_stand` and `explosion` is not.
    #[test]
    fn an_attributed_blast_and_an_unattributed_one_are_different_damage_types() {
        let attributed = default_explosion_damage_source(Some(7), Some(3));
        let unattributed = default_explosion_damage_source(Some(7), None);
        assert_eq!(attributed.damage_type.key.path, "player_explosion");
        assert_eq!(unattributed.damage_type.key.path, "explosion");
    }

    /// A blast with no entity at all -- a respawn anchor, a bed -- is the
    /// ordinary kind, never the attributed one.
    #[test]
    fn a_blast_with_no_entity_behind_it_is_the_ordinary_kind() {
        let source = default_explosion_damage_source(None, None);
        assert_eq!(source.damage_type.key.path, "explosion");
    }

    /// The shooter is answerable for a fireball, but the fireball is what
    /// arrived. Vanilla keeps the two apart, and `DamageSource::is_direct`
    /// reads exactly that difference -- before this pair existed every blast
    /// claimed to be direct.
    #[test]
    fn a_shot_blast_credits_the_shooter_without_claiming_to_be_direct() {
        let source = default_explosion_damage_source(Some(41), Some(9))
            .with_direct_entity(41)
            .with_causing_entity(9);
        assert_eq!(source.direct_entity_id, Some(41));
        assert_eq!(source.causing_entity_id, Some(9));
        assert!(
            !source.is_direct(),
            "a shot blast reported itself as direct"
        );
    }

    /// A creeper is its own cause, so its blast is attributed and direct at
    /// once. This is the case that makes the `player_explosion` name
    /// misleading, and it is worth pinning so nobody "fixes" it back.
    #[test]
    fn a_creeper_is_its_own_cause_and_its_blast_is_both_attributed_and_direct() {
        let source = default_explosion_damage_source(Some(5), Some(5))
            .with_direct_entity(5)
            .with_causing_entity(5);
        assert_eq!(source.damage_type.key.path, "player_explosion");
        assert!(source.is_direct(), "a creeper's own blast was not direct");
    }
}
