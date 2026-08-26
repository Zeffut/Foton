//! Sculk shrieker block-entity storage and shriek response.
//!
//! Vanilla parity: `SculkShriekerBlockEntity`.
//!
//! A shrieker hears through a vibration listener like the sensors do, but its listenable
//! events are only `sculk_sensor_tendrils_clicking`: it answers a sensor that just fired
//! near it, not the footstep the sensor heard.
//!
//! Four warnings summon a warden. The count is not kept here -- it lives on the player,
//! so walking to a different shrieker does not restart it.

use std::sync::{Arc, Weak};

use glam::DVec3;
use rand::random_range;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use steel_registry::game_events::GameEventRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_game_event_tags::GameEventTag;
use steel_registry::vanilla_game_rules::SPAWN_WARDENS;
use steel_registry::{
    level_events, sound_events, vanilla_block_entity_types, vanilla_entities, vanilla_game_events,
};
use steel_utils::types::{Difficulty, UpdateFlags};
use steel_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, Identifier,
    locks::SyncMutex,
};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::entities::{MAX_WARDEN_WARNING_LEVEL, WardenEntity, try_warn_of_warden};
use crate::entity::{Entity, EntitySpawnReason};
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::vibrations::{
    VIBRATION_DATA_TAG, VibrationListener, VibrationPositionSource, VibrationUser,
};
use crate::world::game_event::{GameEventContext, SharedGameEventListener};
use crate::world::spawn_util::SpawnStrategy;

/// Vanilla `SculkShriekerBlockEntity.SHRIEKING_TICKS`.
const SHRIEKING_TICKS: i32 = 90;
/// Vanilla `SculkShriekerBlockEntity.WARNING_SOUND_RADIUS`.
const WARNING_SOUND_RADIUS: i32 = 10;
/// Vanilla `SculkShriekerBlockEntity.DEFAULT_WARNING_LEVEL`.
const DEFAULT_WARNING_LEVEL: i32 = 0;
/// Vanilla `SculkShriekerBlockEntity.VibrationUser.LISTENER_RADIUS`.
const LISTENER_RADIUS: i32 = 8;
/// Vanilla `SculkShriekerBlockEntity.WARDEN_SPAWN_ATTEMPTS`.
const WARDEN_SPAWN_ATTEMPTS: i32 = 20;
/// Vanilla `SculkShriekerBlockEntity.WARDEN_SPAWN_RANGE_XZ`.
const WARDEN_SPAWN_RANGE_XZ: i32 = 5;
/// Vanilla `SculkShriekerBlockEntity.WARDEN_SPAWN_RANGE_Y`.
const WARDEN_SPAWN_RANGE_Y: i32 = 6;
/// Vanilla `SculkShriekerBlockEntity.DARKNESS_RADIUS`.
const DARKNESS_RADIUS: f64 = 40.0;

const SHRIEKING: &BoolProperty = &BlockStateProperties::SHRIEKING;
const CAN_SUMMON: &BoolProperty = &BlockStateProperties::CAN_SUMMON;

struct SculkShriekerState {
    warning_level: i32,
}

/// Vanilla `SculkShriekerBlockEntity`.
pub struct SculkShriekerBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<SculkShriekerState>,
    listener: Arc<VibrationListener>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SculkShriekerBlockEntity`.
unsafe impl DowncastType for SculkShriekerBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/sculk_shrieker");
}

/// Runs `action` with the player behind `source_entity`, if there is one.
///
/// Vanilla parity: `SculkShriekerBlockEntity.tryGetPlayer`, which returns the player
/// itself, the player riding the entity, or the player who fired the projectile. Vanilla
/// also resolves a thrown item's owner; Steel's `ItemEntity` stores that owner as a UUID
/// with no level-side lookup reachable from here, so a shrieker cannot yet be triggered by
/// an item a player threw. A callback avoids handing back a borrow into the temporary
/// `Arc` the passenger and projectile-owner lookups return.
pub fn with_shrieking_player<R>(
    source_entity: &dyn Entity,
    action: impl FnOnce(&Player) -> R,
) -> Option<R> {
    if let Some(player) = source_entity.as_player() {
        return Some(action(player));
    }

    if let Some(passenger) = source_entity.controlling_passenger()
        && let Some(player) = passenger.as_player()
    {
        return Some(action(player));
    }

    let owner = source_entity.as_projectile()?.get_owner()?;
    let player = owner.as_player()?;
    Some(action(player))
}

impl SculkShriekerBlockEntity {
    /// Creates sculk shrieker storage.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let user = Arc::new(SculkShriekerVibrationUser {
            world: Weak::clone(&world),
            block_pos: pos,
        });
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::SCULK_SHRIEKER,
                world,
                pos,
                state,
            ),
            state: SyncMutex::new(SculkShriekerState {
                warning_level: DEFAULT_WARNING_LEVEL,
            }),
            listener: Arc::new(VibrationListener::new(user)),
        }
    }

    /// Returns vanilla `SculkShriekerBlockEntity.getListener`.
    #[must_use]
    pub const fn listener(&self) -> &Arc<VibrationListener> {
        &self.listener
    }

    /// Returns the stored warden warning level.
    #[must_use]
    pub fn warning_level(&self) -> i32 {
        self.state.lock().warning_level
    }

    /// Runs vanilla `SculkShriekerBlockEntity.tryShriek`.
    ///
    /// A shrieker that cannot summon always shrieks; one that can only shrieks when the
    /// player has earned a warning, which is what stops a player farming four shrieks out
    /// of ten seconds on one block.
    pub fn try_shriek(&self, world: &Arc<World>, player: &Player) {
        let state = self.get_block_state();
        if state.get_value(SHRIEKING) {
            return;
        }

        self.state.lock().warning_level = DEFAULT_WARNING_LEVEL;
        if !self.can_respond(world) || self.try_to_warn(world, player) {
            self.shriek(world, player);
        }
    }

    /// Runs vanilla `SculkShriekerBlockEntity.tryToWarn`.
    fn try_to_warn(&self, world: &Arc<World>, player: &Player) -> bool {
        let Some(warning_level) = try_warn_of_warden(world, self.get_block_pos(), player) else {
            return false;
        };
        self.state.lock().warning_level = warning_level;
        true
    }

    /// Runs vanilla `SculkShriekerBlockEntity.shriek`.
    fn shriek(&self, world: &Arc<World>, source: &dyn Entity) {
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        world.set_block(
            pos,
            state.set_value(SHRIEKING, true),
            UpdateFlags::UPDATE_CLIENTS,
        );
        world.schedule_block_tick_default(pos, state.get_block(), SHRIEKING_TICKS);
        world.level_event(level_events::PARTICLES_SCULK_SHRIEK, pos, 0, None);
        world.game_event(
            &vanilla_game_events::SHRIEK,
            pos,
            &GameEventContext::new(Some(source), None),
        );
    }

    /// Returns vanilla `SculkShriekerBlockEntity.canRespond`.
    fn can_respond(&self, world: &Arc<World>) -> bool {
        self.get_block_state().get_value(CAN_SUMMON)
            && world.difficulty() != Difficulty::Peaceful
            && world.get_game_rule(&SPAWN_WARDENS)
    }

    /// Runs vanilla `SculkShriekerBlockEntity.tryRespond`.
    ///
    /// The answer to a shriek is either a warden or the sound of one getting closer, and
    /// either way everybody nearby goes blind.
    pub fn try_respond(&self, world: &Arc<World>) {
        let warning_level = self.warning_level();
        if !self.can_respond(world) || warning_level <= 0 {
            return;
        }

        if !self.try_summon_warden(world, warning_level) {
            self.play_warden_reply_sound(world, warning_level);
        }

        let pos = self.get_block_pos();
        WardenEntity::apply_darkness_around(
            world,
            DVec3::new(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()) + 0.5,
                f64::from(pos.z()) + 0.5,
            ),
            None,
            DARKNESS_RADIUS,
        );
    }

    /// Runs vanilla `SculkShriekerBlockEntity.trySummonWarden`.
    fn try_summon_warden(&self, world: &Arc<World>, warning_level: i32) -> bool {
        if warning_level < MAX_WARDEN_WARNING_LEVEL {
            return false;
        }
        world
            .try_spawn_mob(
                &vanilla_entities::WARDEN,
                EntitySpawnReason::Triggered,
                self.get_block_pos(),
                WARDEN_SPAWN_ATTEMPTS,
                WARDEN_SPAWN_RANGE_XZ,
                WARDEN_SPAWN_RANGE_Y,
                SpawnStrategy::OnTopOfCollider,
            )
            .is_some()
    }

    /// Runs vanilla `SculkShriekerBlockEntity.playWardenReplySound`.
    fn play_warden_reply_sound(&self, world: &Arc<World>, warning_level: i32) {
        let Some(sound) = Self::warden_reply_sound(warning_level) else {
            return;
        };

        let pos = self.get_block_pos();
        let offset = || random_range(-WARNING_SOUND_RADIUS..=WARNING_SOUND_RADIUS);
        // Vanilla plays this at the raw integer coordinates, not the block center.
        let sound_pos = DVec3::new(
            f64::from(pos.x() + offset()),
            f64::from(pos.y() + offset()),
            f64::from(pos.z() + offset()),
        );
        world.play_sound_at(sound, SoundSource::Hostile, sound_pos, 5.0, 1.0, None);
    }

    /// Vanilla parity: `SculkShriekerBlockEntity.SOUND_BY_LEVEL`.
    const fn warden_reply_sound(warning_level: i32) -> Option<SoundEventRef> {
        match warning_level {
            1 => Some(&sound_events::ENTITY_WARDEN_NEARBY_CLOSE),
            2 => Some(&sound_events::ENTITY_WARDEN_NEARBY_CLOSER),
            3 => Some(&sound_events::ENTITY_WARDEN_NEARBY_CLOSEST),
            4 => Some(&sound_events::ENTITY_WARDEN_LISTENING_ANGRY),
            _ => None,
        }
    }
}

impl BlockEntity for SculkShriekerBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `SculkShriekerBlock.getTicker`, which is `VibrationSystem.Ticker.tick`.
    fn tick(&self, world: &Arc<World>) {
        self.listener.tick(world);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        self.state.lock().warning_level = nbt.int("warning_level").unwrap_or(DEFAULT_WARNING_LEVEL);
        self.listener
            .load(nbt.compound(VIBRATION_DATA_TAG).as_ref());
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert("warning_level", self.state.lock().warning_level);
        let mut listener = NbtCompound::new();
        self.listener.save(&mut listener);
        nbt.insert(VIBRATION_DATA_TAG, listener);
    }

    fn game_event_listener(&self) -> Option<SharedGameEventListener> {
        Some(Arc::clone(&self.listener) as SharedGameEventListener)
    }

    /// Vanilla parity: `SculkShriekerBlockEntity.preRemoveSideEffects`.
    ///
    /// A shrieker broken mid-shriek still answers, so mining the block is not a way to
    /// cancel the response that was already coming.
    fn pre_remove_side_effects(&self, _pos: BlockPos, state: BlockStateId) {
        if !state.get_value(SHRIEKING) {
            return;
        }
        let Some(world) = self.get_level() else {
            return;
        };
        self.try_respond(&world);
    }
}

/// Vanilla `SculkShriekerBlockEntity.VibrationUser`.
///
/// Its listenable events are only `sculk_sensor_tendrils_clicking`, so a shrieker answers a
/// sculk sensor going off nearby rather than hearing the player itself.
struct SculkShriekerVibrationUser {
    world: Weak<World>,
    block_pos: BlockPos,
}

impl SculkShriekerVibrationUser {
    fn with_shrieker<R>(&self, action: impl FnOnce(&SculkShriekerBlockEntity) -> R) -> Option<R> {
        let world = self.world.upgrade()?;
        let block_entity = world.get_block_entity(self.block_pos)?;
        let shrieker = block_entity.downcast_ref::<SculkShriekerBlockEntity>()?;
        Some(action(shrieker))
    }
}

impl VibrationUser for SculkShriekerVibrationUser {
    fn listener_radius(&self) -> i32 {
        LISTENER_RADIUS
    }

    fn position_source(&self) -> VibrationPositionSource {
        VibrationPositionSource::Block(self.block_pos)
    }

    fn listenable_events(&self) -> Identifier {
        GameEventTag::SHRIEKER_CAN_LISTEN
    }

    fn requires_adjacent_chunks_to_be_ticking(&self) -> bool {
        true
    }

    /// Vanilla `SculkShriekerBlockEntity.VibrationUser.canReceiveVibration`.
    fn can_receive_vibration(
        &self,
        world: &Arc<World>,
        _pos: BlockPos,
        _event: GameEventRef,
        context: &GameEventContext<'_>,
    ) -> bool {
        if world.get_block_state(self.block_pos).get_value(SHRIEKING) {
            return false;
        }
        context
            .source_entity()
            .and_then(|source| with_shrieking_player(source, |_| ()))
            .is_some()
    }

    /// Vanilla `SculkShriekerBlockEntity.VibrationUser.onReceiveVibration`.
    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        _pos: BlockPos,
        _event: GameEventRef,
        source_entity: Option<&dyn Entity>,
        projectile_owner: Option<&dyn Entity>,
        _receiving_distance: f32,
    ) {
        let Some(source) = projectile_owner.or(source_entity) else {
            return;
        };
        with_shrieking_player(source, |player| {
            self.with_shrieker(|shrieker| shrieker.try_shriek(world, player));
        });
    }

    fn on_data_changed(&self) {
        self.with_shrieker(BlockEntity::set_changed);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;

    fn shrieker() -> SculkShriekerBlockEntity {
        init_vanilla_registry();
        SculkShriekerBlockEntity::new(
            Weak::new(),
            BlockPos::new(-2, 5, 40),
            vanilla_blocks::SCULK_SHRIEKER.default_state(),
        )
    }

    /// The warning level is the only thing that carries a player's history with a
    /// shrieker across a reload; losing it would reset the walk toward a warden
    /// every time the chunk unloaded, and dropping the field would corrupt a
    /// vanilla world that had already counted warnings.
    #[test]
    fn the_warning_level_survives_a_save_and_load() {
        let mut disk = NbtCompound::new();
        disk.insert("warning_level", 3_i32);
        let mut bytes = Vec::new();
        disk.write(&mut bytes);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");

        let loaded = shrieker();
        loaded.load_additional(&borrowed);
        assert_eq!(loaded.warning_level(), 3);

        let mut written = NbtCompound::new();
        loaded.save_additional(&mut written);
        assert_eq!(written.int("warning_level"), Some(3));
    }

    /// A shrieker placed by a player has never warned anyone.
    #[test]
    fn a_shrieker_with_no_stored_data_has_never_warned() {
        let nbt = NbtCompound::new();
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");

        let loaded = shrieker();
        loaded.load_additional(&borrowed);
        assert_eq!(loaded.warning_level(), 0);
    }

    /// Each warning level has its own warden cry, and level four is the one that
    /// means the warden is coming; a wrong mapping would tell the player the
    /// opposite of how much trouble they are in.
    #[test]
    fn each_warning_level_has_its_own_warden_cry() {
        init_vanilla_registry();
        assert!(SculkShriekerBlockEntity::warden_reply_sound(0).is_none());
        assert_eq!(
            SculkShriekerBlockEntity::warden_reply_sound(1),
            Some(&sound_events::ENTITY_WARDEN_NEARBY_CLOSE)
        );
        assert_eq!(
            SculkShriekerBlockEntity::warden_reply_sound(4),
            Some(&sound_events::ENTITY_WARDEN_LISTENING_ANGRY)
        );
        assert!(SculkShriekerBlockEntity::warden_reply_sound(5).is_none());
    }
}
