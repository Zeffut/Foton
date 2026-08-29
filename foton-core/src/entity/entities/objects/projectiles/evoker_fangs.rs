//! Evoker fangs entity.
//!
//! Vanilla parity: `EvokerFangs`. A pair of jaws that rise out of the ground,
//! bite once, and sink. Vanilla files it under projectiles but it is a plain
//! entity: it does not move, it cannot be hurt, and it exists for a little
//! over a second. The two numbers that matter are the warmup -- which is how
//! the evoker's line of fangs ripples outwards instead of firing at once --
//! and the eight-tick offset at which the bite lands, well after the jaws are
//! visible.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::behavior::PushReaction;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_damage_types;
use foton_registry::vanilla_entity_data::EvokerFangsEntityData;
use foton_utils::UuidExt as _;
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use uuid::Uuid;

use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, RemovalReason, SharedEntity,
};
use crate::world::World;

/// NBT key vanilla stores the warmup under.
const TAG_WARMUP: &str = "Warmup";
/// NBT key vanilla stores the owner under.
const TAG_OWNER: &str = "Owner";

/// Ticks the jaws stay up once they have risen.
///
/// Vanilla parity: the `lifeTicks = 22` field.
const LIFE_TICKS: i32 = 22;

/// Warmup value at which the bite lands.
///
/// Vanilla parity: the `warmupDelayTicks == -8` of `tick`. The jaws are drawn
/// from the moment the warmup runs out, so the bite is eight ticks behind what
/// a player sees -- which is the window to walk out of them.
const BITE_AT_WARMUP: i32 = -8;

/// How far past the jaws the bite reaches.
///
/// Vanilla parity: the `inflate(0.2, 0.0, 0.2)` of `tick`.
const BITE_REACH: f64 = 0.2;

/// Damage one bite deals.
///
/// Vanilla parity: the `6.0F` of `dealDamageTo`.
const BITE_DAMAGE: f32 = 6.0;

/// A pair of evoker fangs.
#[entity_behavior(class = "EvokerFangs")]
pub struct EvokerFangsEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<EvokerFangsEntityData>,
    state: SyncMutex<FangsState>,
}

/// The three counters a pair of fangs carries.
#[derive(Debug)]
struct FangsState {
    /// Ticks before the jaws rise; counts down past zero to time the bite.
    warmup_delay_ticks: i32,
    /// Whether the client has been told to draw the jaws.
    sent_spike_event: bool,
    /// Ticks the jaws stay up for once risen.
    life_ticks: i32,
    /// Who cast the spell, so the fangs do not bite their own side.
    owner: Option<Uuid>,
}

impl FangsState {
    const fn new() -> Self {
        Self {
            warmup_delay_ticks: 0,
            sent_spike_event: false,
            life_ticks: LIFE_TICKS,
            owner: None,
        }
    }
}

// SAFETY: This key is owned by Foton and uniquely identifies `EvokerFangsEntity`.
unsafe impl DowncastType for EvokerFangsEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/evoker_fangs");
}

impl EvokerFangsEntity {
    /// Creates a pair of fangs at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(EvokerFangsEntityData::new()),
            state: SyncMutex::new(FangsState::new()),
        }
    }

    /// Creates a pair of fangs from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(EvokerFangsEntityData::new()),
            state: SyncMutex::new(FangsState::new()),
        }
    }

    /// Aims a fresh pair of fangs, the way the evoker's spell does.
    ///
    /// Vanilla parity: the `EvokerFangs(level, x, y, z, rotation, warmup, owner)`
    /// constructor. The rotation arrives in radians, as the spell computes it.
    pub fn place(&self, position: DVec3, rotation_radians: f32, warmup_delay_ticks: i32) {
        {
            let mut state = self.state.lock();
            state.warmup_delay_ticks = warmup_delay_ticks;
        }
        self.base().set_position_local(position);
        self.set_rotation((rotation_radians.to_degrees(), 0.0));
        self.base().set_old_rotation_to_current();
    }

    /// Sets who cast the spell.
    pub fn set_owner_uuid(&self, owner: Option<Uuid>) {
        self.state.lock().owner = owner;
    }

    /// Returns who cast the spell.
    #[must_use]
    pub fn owner_uuid(&self) -> Option<Uuid> {
        self.state.lock().owner
    }

    /// Resolves the caster in the current world.
    ///
    /// Vanilla parity: `EvokerFangs.getOwner`.
    #[must_use]
    pub fn get_owner(&self) -> Option<SharedEntity> {
        let uuid = self.owner_uuid()?;
        self.level()?.get_entity_by_uuid(&uuid)
    }

    /// Bites everything standing in the jaws.
    ///
    /// Vanilla parity: the `warmupDelayTicks == -8` branch of `tick`.
    fn bite(&self, world: &World) {
        let owner = self.get_owner();
        let owner_id = owner.as_ref().map(|owner| owner.id());
        let source = match owner_id {
            // Vanilla parity: `damageSources().indirectMagic(this, owner)`.
            Some(owner_id) => DamageSource::environment(&vanilla_damage_types::INDIRECT_MAGIC)
                .with_causing_entity(owner_id)
                .with_direct_entity(self.id()),
            // Vanilla parity: `damageSources().magic()` when the caster is gone.
            None => DamageSource::environment(&vanilla_damage_types::MAGIC),
        };

        let bite_box = self.bounding_box().inflate_xyz(BITE_REACH, 0.0, BITE_REACH);
        let caught = world.get_entities_in_aabb_matching(&bite_box, |entity| {
            let Some(living) = entity.as_living_entity() else {
                return false;
            };
            LivingEntity::is_alive(living)
                && !entity.is_invulnerable()
                && Some(entity.id()) != owner_id
        });

        for entity in caught {
            // Vanilla parity: an owned pair of fangs never bites the caster's
            // allies, which is what keeps an evoker's fangs off its vexes.
            if let Some(owner) = owner.as_ref()
                && owner.is_allied_to(entity.as_ref())
            {
                continue;
            }
            let Some(living) = entity.as_living_entity() else {
                continue;
            };
            living.hurt(world, &source, BITE_DAMAGE);
        }
    }

    /// Runs vanilla `EvokerFangs.tick` on the server side.
    fn tick_server(&self, world: &World) {
        let (should_bite, should_spike, expired) = {
            let mut state = self.state.lock();
            state.warmup_delay_ticks -= 1;
            if state.warmup_delay_ticks >= 0 {
                return;
            }

            let should_bite = state.warmup_delay_ticks == BITE_AT_WARMUP;
            let should_spike = !state.sent_spike_event;
            state.sent_spike_event = true;
            state.life_ticks -= 1;
            (should_bite, should_spike, state.life_ticks < 0)
        };

        if should_bite {
            self.bite(world);
        }
        if should_spike {
            self.broadcast_entity_event(EntityStatus::StartAttacking);
        }
        if expired {
            self.set_removed(RemovalReason::Killed);
        }
    }
}

impl Entity for EvokerFangsEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn tick(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.tick_server(&world);
    }

    /// Vanilla parity: `EvokerFangs.hurtServer` returns false, so the jaws
    /// cannot be destroyed before they bite.
    fn hurt(&self, _world: &World, _source: &DamageSource, _amount: f32) -> bool {
        false
    }

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        false
    }

    fn could_accept_passenger(&self) -> bool {
        false
    }

    fn piston_push_reaction(&self) -> PushReaction {
        PushReaction::Ignore
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        nbt.insert(TAG_WARMUP, state.warmup_delay_ticks);
        if let Some(owner) = state.owner {
            nbt.insert(TAG_OWNER, NbtTag::IntArray(owner.to_int_array().to_vec()));
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let mut state = self.state.lock();
        state.warmup_delay_ticks = nbt.int(TAG_WARMUP).unwrap_or(0);
        state.owner = nbt
            .int_array(TAG_OWNER)
            .and_then(|owner| Uuid::from_int_array(&owner));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use foton_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::next_entity_id;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

    /// The bite is eight ticks behind the jaws a player sees, and the whole
    /// pair lives for the warmup plus thirty ticks. Both numbers are what make
    /// an evoker's fang line dodgeable rather than instant.
    #[test]
    fn fangs_bite_eight_ticks_after_they_appear_and_sink_thirty_ticks_later() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("evoker_fangs_lifetime");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let fangs = Arc::new(EvokerFangsEntity::new(
            &vanilla_entities::EVOKER_FANGS,
            next_entity_id(),
            TEST_POSITION,
            Arc::downgrade(&world),
        ));
        fangs.place(TEST_POSITION, 0.0, 0);
        world
            .try_add_entity(Arc::clone(&fangs) as SharedEntity)
            .expect("the fangs should attach to the loaded chunk");

        // One tick takes the warmup to -1, which is when the jaws are drawn.
        fangs.tick();
        assert!(fangs.state.lock().sent_spike_event);
        assert_eq!(fangs.state.lock().life_ticks, LIFE_TICKS - 1);

        for _ in 0..(-BITE_AT_WARMUP - 1) {
            fangs.tick();
        }
        assert_eq!(fangs.state.lock().warmup_delay_ticks, BITE_AT_WARMUP);

        for _ in 0..LIFE_TICKS {
            fangs.tick();
        }
        assert!(fangs.is_removed(), "the jaws sink once their life runs out");
    }
}
