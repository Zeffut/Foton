//! Explosions.
//!
//! Vanilla parity: `ServerExplosion`. An explosion casts rays out of its center,
//! spends power against the blocks it crosses, damages and pushes nearby
//! entities by how much of them the blast can see, and optionally leaves fire.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::{vanilla_attributes, vanilla_damage_types};
use steel_utils::{BlockPos, WorldAabb};

use crate::entity::damage::DamageSource;
use crate::world::World;
use crate::world::raycast::{ClipBlockShape, ClipFluid};

/// What an explosion does to the blocks it reaches.
///
/// Vanilla parity: `Explosion.BlockInteraction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplosionBlockInteraction {
    /// Leaves every block standing.
    Keep,
    /// Breaks blocks and drops their loot.
    Destroy,
}

/// Rays are cast from a 16x16x16 grid, using only its outer shell.
const RAY_GRID_SIZE: i32 = 16;
/// Distance each ray advances per step.
const RAY_STEP: f32 = 0.3;
/// Power every ray loses per step regardless of what it crosses.
const RAY_STEP_COST: f32 = 0.225_000_01;

impl World {
    /// Detonates an explosion and returns the positions it destroyed.
    ///
    /// Vanilla parity: `ServerExplosion.explode`.
    pub fn explode(
        self: &Arc<Self>,
        source_entity_id: Option<i32>,
        damage_source: Option<DamageSource>,
        center: DVec3,
        radius: f32,
        fire: bool,
        interaction: ExplosionBlockInteraction,
    ) -> Vec<BlockPos> {
        let to_blow = self.calculate_exploded_positions(center, radius);
        self.hurt_entities_from_explosion(source_entity_id, damage_source, center, radius);

        if interaction == ExplosionBlockInteraction::Destroy {
            for pos in &to_blow {
                self.destroy_block(*pos, true);
            }
        }

        if fire {
            self.create_explosion_fire(&to_blow);
        }

        to_blow
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
        state: steel_utils::BlockStateId,
    ) -> Option<f32> {
        let fluid = crate::fluid::get_fluid_state(self, pos);
        if state.is_air() && fluid.is_empty() {
            return None;
        }
        let block_resistance = state.get_block().config.explosion_resistance;
        let fluid_resistance = fluid.fluid_id.explosion_resistance;
        Some(block_resistance.max(fluid_resistance))
    }

    /// Damages and pushes every entity the blast reaches.
    ///
    /// Vanilla parity: `ServerExplosion.hurtEntities`.
    fn hurt_entities_from_explosion(
        self: &Arc<Self>,
        source_entity_id: Option<i32>,
        damage_source: Option<DamageSource>,
        center: DVec3,
        radius: f32,
    ) {
        if radius < 1.0e-5 {
            return;
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

        let mut damage_source = damage_source
            .unwrap_or_else(|| DamageSource::environment(&vanilla_damage_types::EXPLOSION));
        damage_source = damage_source.with_source_position(center);
        if let Some(id) = source_entity_id {
            damage_source = damage_source.with_causing_entity(id).with_direct_entity(id);
        }

        for entity in self.get_entities_in_aabb(&aabb) {
            if Some(entity.id()) == source_entity_id {
                continue;
            }
            let distance = entity.position().distance(center) / reach;
            if distance > 1.0 {
                continue;
            }

            let exposure = f64::from(self.seen_percent(center, entity.as_ref()));

            // Vanilla parity: `ExplosionDamageCalculator.getEntityDamageAmount`.
            let impact = (1.0 - distance) * exposure;
            let damage = (impact.mul_add(impact, impact) / 2.0).mul_add(7.0 * reach, 1.0);
            entity.hurt(self, &damage_source, damage as f32);

            // Vanilla pushes from the blast toward the entity's eyes.
            let origin = entity.position().with_y(entity.get_eye_y());
            let direction = origin - center;
            if direction.length_squared() > 0.0 {
                let resistance = entity.as_living_entity().map_or(0.0, |living| {
                    living
                        .attributes()
                        .lock()
                        .required_value(&vanilla_attributes::EXPLOSION_KNOCKBACK_RESISTANCE)
                });
                let power = impact * (1.0 - resistance);
                let knockback = direction.normalize() * power;
                entity.set_velocity(entity.velocity() + knockback);
                entity.mark_velocity_sync();
            }
        }
    }

    /// Returns the fraction of an entity the blast can see, from 0 to 1.
    ///
    /// Vanilla parity: `ServerExplosion.getSeenPercent`. Vanilla samples a grid of
    /// points across the entity's bounding box and counts how many reach the center
    /// without crossing a block.
    fn seen_percent(&self, center: DVec3, entity: &dyn crate::entity::Entity) -> f32 {
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
                steel_registry::vanilla_blocks::FIRE.default_state(),
                steel_utils::types::UpdateFlags::UPDATE_ALL,
            );
        }
    }
}
