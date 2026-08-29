use std::f32::consts::PI;
use std::sync::Arc;

use foton_utils::{BlockPos, Downcast as _, UuidExt as _};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use uuid::Uuid;

use crate::entity::entities::LeashFenceKnotEntity;
use crate::entity::{Entity, Mob, SharedEntity, WeakEntity};

pub(super) const LEASH_SNAP_DISTANCE: f64 = 12.0;
pub(super) const LEASH_ELASTIC_DISTANCE: f64 = 6.0;
pub(super) const LEASH_AXIS_SPECIFIC_ELASTICITY: DVec3 = DVec3::new(0.8, 0.2, 0.8);
pub(super) const LEASH_SPRING_DAMPENING: f64 = 0.7;
pub(super) const LEASH_TORSIONAL_ELASTICITY: f64 = 10.0;
pub(super) const LEASH_STIFFNESS: f64 = 0.11;
/// Where a single lead meets the leashed entity.
///
/// Vanilla parity: `Leashable.ENTITY_ATTACHMENT_POINT`, a one-element list.
pub(super) const ENTITY_LEASH_ATTACHMENT_POINT: [DVec3; 1] = [DVec3::new(0.0, 0.5, 0.5)];
/// Where a single lead meets its holder.
///
/// Vanilla parity: `Leashable.LEASHER_ATTACHMENT_POINT`, a one-element list.
pub(super) const LEASHER_ATTACHMENT_POINT: [DVec3; 1] = [DVec3::new(0.0, 0.5, 0.0)];
/// The four corners a quad leash ties to, used on both ends of the ropes.
///
/// Vanilla parity: `Leashable.SHARED_QUAD_ATTACHMENT_POINTS`.
pub(super) const SHARED_QUAD_ATTACHMENT_POINTS: [DVec3; 4] = [
    DVec3::new(-0.5, 0.5, 0.5),
    DVec3::new(-0.5, 0.5, -0.5),
    DVec3::new(0.5, 0.5, -0.5),
    DVec3::new(0.5, 0.5, 0.5),
];
/// Share of the accumulated pull one quad leash rope carries.
///
/// Vanilla parity: the `scale(quadConnection ? 0.25 : 1.0)` of
/// `Leashable.checkElasticInteractions`. Without it four ropes would yank four
/// times as hard as one.
pub(super) const QUAD_LEASH_WRENCH_SCALE: f64 = 0.25;
pub(super) const DELAYED_LEASH_DROP_TICKS: i32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeashAttachment {
    Entity(Uuid),
    FenceKnot(BlockPos),
}

#[derive(Debug, Clone)]
pub(super) struct LeashData {
    pub(super) attachment: LeashAttachment,
    pub(super) holder: Option<WeakEntity>,
    pub(super) angular_momentum: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LeashWrench {
    pub(super) force: DVec3,
    pub(super) torque: f64,
}

impl LeashWrench {
    pub(super) const fn new(force: DVec3, torque: f64) -> Self {
        Self { force, torque }
    }

    /// Vanilla parity: `Leashable.Wrench.accumulate`, folded one rope at a time.
    fn accumulate(self, other: Self) -> Self {
        Self::new(self.force + other.force, self.torque + other.torque)
    }

    /// Vanilla parity: `Leashable.Wrench.scale`.
    pub(super) fn scale(self, scale: f64) -> Self {
        Self::new(self.force * scale, self.torque * scale)
    }
}

impl LeashData {
    pub(super) fn from_entity(holder: &SharedEntity) -> Self {
        let attachment = holder.downcast_ref::<LeashFenceKnotEntity>().map_or_else(
            || LeashAttachment::Entity(holder.uuid()),
            |knot| LeashAttachment::FenceKnot(knot.block_pos()),
        );
        Self {
            attachment,
            holder: Some(Arc::downgrade(holder)),
            angular_momentum: 0.0,
        }
    }

    pub(super) const fn from_delayed_attachment(attachment: LeashAttachment) -> Self {
        Self {
            attachment,
            holder: None,
            angular_momentum: 0.0,
        }
    }

    pub(super) fn holder(&self) -> Option<SharedEntity> {
        self.holder.as_ref().and_then(WeakEntity::upgrade)
    }

    pub(super) fn saved_attachment(&self) -> LeashAttachment {
        self.holder().map_or(self.attachment, |holder| {
            holder.downcast_ref::<LeashFenceKnotEntity>().map_or_else(
                || LeashAttachment::Entity(holder.uuid()),
                |knot| LeashAttachment::FenceKnot(knot.block_pos()),
            )
        })
    }

    pub(super) fn set_holder(&mut self, holder: &SharedEntity) {
        self.attachment = holder.downcast_ref::<LeashFenceKnotEntity>().map_or_else(
            || LeashAttachment::Entity(holder.uuid()),
            |knot| LeashAttachment::FenceKnot(knot.block_pos()),
        );
        self.holder = Some(Arc::downgrade(holder));
        self.angular_momentum = 0.0;
    }

    pub(super) fn save(&self, nbt: &mut NbtCompound) {
        match self.saved_attachment() {
            LeashAttachment::Entity(uuid) => {
                let mut leash = NbtCompound::new();
                leash.insert("UUID", NbtTag::IntArray(uuid.to_int_array().to_vec()));
                nbt.insert("leash", NbtTag::Compound(leash));
            }
            LeashAttachment::FenceKnot(pos) => {
                nbt.insert("leash", NbtTag::IntArray(vec![pos.x(), pos.y(), pos.z()]));
            }
        }
    }

    pub(super) fn load(nbt: BorrowedNbtCompoundView<'_, '_>) -> Option<Self> {
        if let Some(leash) = nbt.compound("leash")
            && let Some(uuid_array) = leash.int_array("UUID")
            && let Some(uuid) = Uuid::from_int_array(&uuid_array)
        {
            return Some(Self::from_delayed_attachment(LeashAttachment::Entity(uuid)));
        }

        nbt.int_array("leash")
            .filter(|position| position.len() == 3)
            .map(|position| {
                Self::from_delayed_attachment(LeashAttachment::FenceKnot(BlockPos::new(
                    position[0],
                    position[1],
                    position[2],
                )))
            })
    }
}

pub(super) fn leash_dimensions(entity: &dyn Entity) -> DVec3 {
    let dimensions = entity.base().dimensions();
    DVec3::new(
        f64::from(dimensions.width),
        f64::from(dimensions.height),
        f64::from(dimensions.width),
    )
}

pub(super) fn leash_bounding_box_center(entity: &dyn Entity) -> DVec3 {
    let bounding_box = entity.bounding_box();
    DVec3::new(
        f64::midpoint(bounding_box.min_x(), bounding_box.max_x()),
        f64::midpoint(bounding_box.min_y(), bounding_box.max_y()),
        f64::midpoint(bounding_box.min_z(), bounding_box.max_z()),
    )
}

pub(super) fn leash_holder_movement(entity: &dyn Entity) -> DVec3 {
    if entity.as_mob().is_some_and(Mob::is_no_ai) {
        return DVec3::ZERO;
    }

    entity.known_movement()
}

pub(super) fn rotate_y(vector: DVec3, radians: f32) -> DVec3 {
    let cos = f64::from(radians.cos());
    let sin = f64::from(radians.sin());
    DVec3::new(
        vector.x * cos + vector.z * sin,
        vector.y,
        vector.z * cos - vector.x * sin,
    )
}

pub(super) fn axis_specific_leash_elasticity(force: DVec3) -> DVec3 {
    force * LEASH_AXIS_SPECIFIC_ELASTICITY
}

/// Sums the pull of every rope tying `entity` to `holder`.
///
/// Vanilla parity: `Leashable.computeElasticInteraction`, which walks a list of
/// attachment points and collects one wrench per taut rope. Returning the
/// accumulated wrench instead of the list keeps vanilla's meaning: `None` is
/// vanilla's empty list, which `checkElasticInteractions` reads as "no rope is
/// pulling".
pub(super) fn compute_elastic_interaction(
    entity: &dyn Entity,
    holder: &dyn Entity,
    slack_distance: f64,
    entity_attachment_points: &[DVec3],
    leasher_attachment_points: &[DVec3],
) -> Option<LeashWrench> {
    let current_movement = leash_holder_movement(entity);
    let entity_y_rot = entity.rotation().0 * PI / 180.0;
    let entity_dimensions = leash_dimensions(entity);
    let holder_y_rot = holder.rotation().0 * PI / 180.0;
    let holder_dimensions = leash_dimensions(holder);

    let mut accumulated: Option<LeashWrench> = None;
    for (entity_point, leasher_point) in entity_attachment_points
        .iter()
        .zip(leasher_attachment_points)
    {
        let entity_attach_vector = rotate_y(*entity_point * entity_dimensions, -entity_y_rot);
        let entity_attach_pos = entity.position() + entity_attach_vector;
        let leasher_attach_vector = rotate_y(*leasher_point * holder_dimensions, -holder_y_rot);
        let leasher_attach_pos = holder.position() + leasher_attach_vector;

        let Some(wrench) = compute_dampened_spring_interaction(
            leasher_attach_pos,
            entity_attach_pos,
            slack_distance,
            current_movement,
            entity_attach_vector,
        ) else {
            continue;
        };

        accumulated = Some(accumulated.map_or(wrench, |total| total.accumulate(wrench)));
    }

    accumulated
}

pub(super) fn compute_dampened_spring_interaction(
    pivot_point: DVec3,
    object_position: DVec3,
    spring_slack: f64,
    object_motion: DVec3,
    lever_arm: DVec3,
) -> Option<LeashWrench> {
    let distance = object_position.distance(pivot_point);
    if distance < spring_slack {
        return None;
    }

    let mut displacement = (pivot_point - object_position).normalize() * (distance - spring_slack);
    let torque = torque_from_force(lever_arm, displacement);
    if object_motion.dot(displacement) >= 0.0 {
        displacement *= 1.0 - LEASH_SPRING_DAMPENING;
    }

    Some(LeashWrench::new(displacement, torque))
}

pub(super) fn torque_from_force(lever_arm: DVec3, force: DVec3) -> f64 {
    lever_arm.z * force.x - lever_arm.x * force.z
}
