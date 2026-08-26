//! Brain memories: the typed key, the slot that expires, and the map of both.

mod nearest_visible;
mod value;
mod walk_target;

use std::fmt;
use std::marker::PhantomData;

use glam::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};
use simdnbt::borrow::{NbtCompound as BorrowedNbtCompound, NbtTag as BorrowedNbtTag};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_utils::{BlockPos, GlobalPos};
use uuid::Uuid;

pub use nearest_visible::NearestVisibleLivingEntities;
pub use value::{EntityMemory, MemoryValue, MemoryValueType, Unit};
pub use walk_target::WalkTarget;

use super::position_tracker::PositionTracker;
use crate::entity::ai::path::Path;
use crate::entity::damage::DamageSource;

/// A memory that never expires on its own.
///
/// Vanilla parity: the `MemorySlot.NEVER_EXPIRE` sentinel, `Long.MAX_VALUE`.
const NEVER_EXPIRE: i64 = i64::MAX;

/// The untyped half of a [`MemoryModuleType`], used as the map key.
///
/// Vanilla keys its memory map on the `MemoryModuleType` instance itself.
/// Steel keys on the registry path because the type is generic and cannot be
/// erased into a map key without losing the payload type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryModuleId(&'static str);

impl MemoryModuleId {
    /// Returns the registry key vanilla registers this memory under.
    #[must_use]
    pub const fn key(self) -> &'static str {
        self.0
    }
}

/// A typed handle on one kind of brain memory.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.memory.MemoryModuleType<U>`.
///
/// Vanilla builds these into `BuiltInRegistries.MEMORY_MODULE_TYPE`, but the
/// list is hardcoded in Java: nothing can add to it, no packet carries a memory
/// id and the save format writes the registry *key*, not its index. Steel
/// therefore mirrors the Java constants directly (see [`memory_module_types`])
/// rather than adding a registry, and `SteelExtractor` emits no
/// `memory_module_type` asset to build one from.
pub struct MemoryModuleType<T: MemoryValueType> {
    key: &'static str,
    serializable: bool,
    marker: PhantomData<fn() -> T>,
}

impl<T: MemoryValueType> MemoryModuleType<T> {
    const fn new(key: &'static str, serializable: bool) -> Self {
        Self {
            key,
            serializable,
            marker: PhantomData,
        }
    }

    /// Returns the untyped key this memory is stored under.
    #[must_use]
    pub const fn id(self) -> MemoryModuleId {
        MemoryModuleId(self.key)
    }

    /// Returns whether vanilla registers this memory with a codec.
    ///
    /// Vanilla parity: `MemoryModuleType.canSerialize`.
    #[must_use]
    pub const fn can_serialize(self) -> bool {
        self.serializable
    }
}

#[expect(
    clippy::expl_impl_clone_on_copy,
    reason = "deriving would add a `T: Clone` bound the phantom type does not need"
)]
impl<T: MemoryValueType> Clone for MemoryModuleType<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: MemoryValueType> Copy for MemoryModuleType<T> {}

impl<T: MemoryValueType> fmt::Debug for MemoryModuleType<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key)
    }
}

/// What a behavior or activity requires of one memory before it may run.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.memory.MemoryStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryStatus {
    /// The memory must hold a value.
    ValuePresent,
    /// The memory must be registered and empty.
    ValueAbsent,
    /// The memory only has to be registered.
    Registered,
}

/// One memory's value and its remaining lifetime.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.memory.MemorySlot`.
#[derive(Debug)]
struct MemorySlot {
    value: Option<MemoryValue>,
    time_to_live: i64,
    serializable: bool,
}

impl MemorySlot {
    const fn empty(serializable: bool) -> Self {
        Self {
            value: None,
            time_to_live: NEVER_EXPIRE,
            serializable,
        }
    }

    /// Vanilla parity: `MemorySlot.tick`.
    fn tick(&mut self) {
        if self.value.is_none() || self.time_to_live == NEVER_EXPIRE {
            return;
        }
        if self.time_to_live <= 0 {
            self.clear();
        } else {
            self.time_to_live -= 1;
        }
    }

    fn clear(&mut self) {
        self.value = None;
        self.time_to_live = NEVER_EXPIRE;
    }

    const fn has_value(&self) -> bool {
        self.value.is_some()
    }
}

/// Reads one memory value back out of NBT.
type MemoryReader = fn(&BorrowedNbtTag<'_, '_>) -> Option<MemoryValue>;

/// Declares the vanilla `MemoryModuleType` constants and their NBT readers.
///
/// The two have to be produced together: the reader table maps a saved registry
/// key back to the value shape, and hand-maintaining it beside the constants is
/// exactly the drift this macro removes.
macro_rules! memory_module_types {
    ($(
        $(#[$attr:meta])*
        $name:ident: $ty:ty = $key:literal, saved = $saved:literal;
    )*) => {
        /// The memory kinds vanilla registers, transcribed from
        /// `net.minecraft.world.entity.ai.memory.MemoryModuleType`.
        ///
        /// Only the memories Steel's ported sensors, behaviors and mobs
        /// actually use are declared; vanilla registers around a hundred, and
        /// the rest arrive with the mobs that read them.
        pub mod memory_module_types {
            use super::{
                BlockPos, DVec3, DamageSource, EntityMemory, FxHashSet, GlobalPos,
                MemoryModuleType, NearestVisibleLivingEntities, Path, PositionTracker, Unit, Uuid,
                WalkTarget,
            };

            $(
                $(#[$attr])*
                pub const $name: MemoryModuleType<$ty> = MemoryModuleType::new($key, $saved);
            )*
        }

        /// Every declared memory, with whether vanilla saves it and how to
        /// read it back.
        const MEMORY_TABLE: &[(&str, bool, MemoryReader)] = &[
            $( ($key, $saved, <$ty as MemoryValueType>::from_nbt), )*
        ];
    };
}

/// Returns whether vanilla registers the memory under this key with a codec.
///
/// The typed constant carries the answer, but a behavior hands the brain back
/// an untyped [`MemoryModuleId`], so registration looks it up here instead.
#[must_use]
pub(super) fn is_saved(memory: MemoryModuleId) -> bool {
    MEMORY_TABLE
        .iter()
        .any(|&(key, saved, _)| saved && key == memory.key())
}

memory_module_types! {
    /// Vanilla `MemoryModuleType.HOME`.
    HOME: GlobalPos = "minecraft:home", saved = true;
    /// Vanilla `MemoryModuleType.JOB_SITE`, the workstation a villager holds a
    /// POI ticket on.
    JOB_SITE: GlobalPos = "minecraft:job_site", saved = true;
    /// Vanilla `MemoryModuleType.POTENTIAL_JOB_SITE`, a workstation an
    /// unemployed villager has claimed but not yet reached.
    POTENTIAL_JOB_SITE: GlobalPos = "minecraft:potential_job_site", saved = true;
    /// Vanilla `MemoryModuleType.MEETING_POINT`, the village bell.
    MEETING_POINT: GlobalPos = "minecraft:meeting_point", saved = true;
    /// Vanilla `MemoryModuleType.SECONDARY_JOB_SITE`, the farmland a farmer
    /// works besides its composter.
    SECONDARY_JOB_SITE: Vec<GlobalPos> = "minecraft:secondary_job_site", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_BED`.
    NEAREST_BED: BlockPos = "minecraft:nearest_bed", saved = false;
    /// Vanilla `MemoryModuleType.HIDING_PLACE`.
    HIDING_PLACE: GlobalPos = "minecraft:hiding_place", saved = false;
    /// Vanilla `MemoryModuleType.HEARD_BELL_TIME`.
    HEARD_BELL_TIME: i64 = "minecraft:heard_bell_time", saved = false;
    /// Vanilla `MemoryModuleType.LAST_SLEPT`, which is also what decides
    /// whether a village has been awake long enough to want an iron golem.
    LAST_SLEPT: i64 = "minecraft:last_slept", saved = true;
    /// Vanilla `MemoryModuleType.LAST_WOKEN`.
    LAST_WOKEN: i64 = "minecraft:last_woken", saved = true;
    /// Vanilla `MemoryModuleType.LAST_WORKED_AT_POI`, the cooldown between two
    /// stints at a workstation.
    LAST_WORKED_AT_POI: i64 = "minecraft:last_worked_at_poi", saved = true;
    /// Vanilla `MemoryModuleType.GOLEM_DETECTED_RECENTLY`.
    GOLEM_DETECTED_RECENTLY: bool = "minecraft:golem_detected_recently", saved = true;
    /// Vanilla `MemoryModuleType.VISIBLE_VILLAGER_BABIES`.
    VISIBLE_VILLAGER_BABIES: Vec<EntityMemory> = "minecraft:visible_villager_babies", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_LIVING_ENTITIES`, registry key `mobs`.
    NEAREST_LIVING_ENTITIES: Vec<EntityMemory> = "minecraft:mobs", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_LIVING_ENTITIES`, registry key `visible_mobs`.
    NEAREST_VISIBLE_LIVING_ENTITIES: NearestVisibleLivingEntities = "minecraft:visible_mobs", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_PLAYERS`.
    NEAREST_PLAYERS: Vec<EntityMemory> = "minecraft:nearest_players", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_PLAYER`.
    NEAREST_VISIBLE_PLAYER: EntityMemory = "minecraft:nearest_visible_player", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_ATTACKABLE_PLAYER`.
    NEAREST_VISIBLE_ATTACKABLE_PLAYER: EntityMemory = "minecraft:nearest_visible_targetable_player", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_ATTACKABLE_PLAYERS`.
    NEAREST_VISIBLE_ATTACKABLE_PLAYERS: Vec<EntityMemory> = "minecraft:nearest_visible_targetable_players", saved = false;
    /// Vanilla `MemoryModuleType.WALK_TARGET`.
    WALK_TARGET: WalkTarget = "minecraft:walk_target", saved = false;
    /// Vanilla `MemoryModuleType.LOOK_TARGET`.
    LOOK_TARGET: PositionTracker = "minecraft:look_target", saved = false;
    /// Vanilla `MemoryModuleType.ATTACK_TARGET`.
    ATTACK_TARGET: EntityMemory = "minecraft:attack_target", saved = false;
    /// Vanilla `MemoryModuleType.ATTACK_COOLING_DOWN`.
    ATTACK_COOLING_DOWN: bool = "minecraft:attack_cooling_down", saved = false;
    /// Vanilla `MemoryModuleType.ATTACK_TARGET_COOLDOWN`, the long pause a
    /// nautilus takes between picking fights nobody started.
    ATTACK_TARGET_COOLDOWN: i32 = "minecraft:attack_target_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.CHARGE_COOLDOWN_TICKS`, what keeps a charging
    /// mob from charging again the moment it lands one.
    CHARGE_COOLDOWN_TICKS: i32 = "minecraft:charge_cooldown_ticks", saved = true;
    /// Vanilla `MemoryModuleType.INTERACTION_TARGET`.
    INTERACTION_TARGET: EntityMemory = "minecraft:interaction_target", saved = false;
    /// Vanilla `MemoryModuleType.PATH`.
    PATH: Path = "minecraft:path", saved = false;
    /// Vanilla `MemoryModuleType.DOORS_TO_CLOSE`.
    DOORS_TO_CLOSE: FxHashSet<GlobalPos> = "minecraft:doors_to_close", saved = false;
    /// Vanilla `MemoryModuleType.HURT_BY`.
    HURT_BY: DamageSource = "minecraft:hurt_by", saved = false;
    /// Vanilla `MemoryModuleType.HURT_BY_ENTITY`.
    HURT_BY_ENTITY: EntityMemory = "minecraft:hurt_by_entity", saved = false;
    /// Vanilla `MemoryModuleType.AVOID_TARGET`.
    AVOID_TARGET: EntityMemory = "minecraft:avoid_target", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_HOSTILE`.
    NEAREST_HOSTILE: EntityMemory = "minecraft:nearest_hostile", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_ATTACKABLE`.
    NEAREST_ATTACKABLE: EntityMemory = "minecraft:nearest_attackable", saved = false;
    /// Vanilla `MemoryModuleType.CANT_REACH_WALK_TARGET_SINCE`.
    CANT_REACH_WALK_TARGET_SINCE: i64 = "minecraft:cant_reach_walk_target_since", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_ADULT`.
    NEAREST_VISIBLE_ADULT: EntityMemory = "minecraft:nearest_visible_adult", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_WANTED_ITEM`.
    NEAREST_VISIBLE_WANTED_ITEM: EntityMemory = "minecraft:nearest_visible_wanted_item", saved = false;
    /// Vanilla `MemoryModuleType.TEMPTING_PLAYER`.
    TEMPTING_PLAYER: EntityMemory = "minecraft:tempting_player", saved = false;
    /// Vanilla `MemoryModuleType.TEMPTATION_COOLDOWN_TICKS`.
    TEMPTATION_COOLDOWN_TICKS: i32 = "minecraft:temptation_cooldown_ticks", saved = true;
    /// Vanilla `MemoryModuleType.GAZE_COOLDOWN_TICKS`.
    GAZE_COOLDOWN_TICKS: i32 = "minecraft:gaze_cooldown_ticks", saved = true;
    /// Vanilla `MemoryModuleType.IS_TEMPTED`.
    IS_TEMPTED: bool = "minecraft:is_tempted", saved = true;
    /// Vanilla `MemoryModuleType.IS_IN_WATER`.
    IS_IN_WATER: Unit = "minecraft:is_in_water", saved = true;
    /// Vanilla `MemoryModuleType.IS_PANICKING`.
    IS_PANICKING: bool = "minecraft:is_panicking", saved = true;
    /// Vanilla `MemoryModuleType.VISITED_BLOCK_POSITIONS`.
    VISITED_BLOCK_POSITIONS: FxHashSet<GlobalPos> = "minecraft:visited_block_positions", saved = true;
    /// Vanilla `MemoryModuleType.UNREACHABLE_TRANSPORT_BLOCK_POSITIONS`.
    UNREACHABLE_TRANSPORT_BLOCK_POSITIONS: FxHashSet<GlobalPos> = "minecraft:unreachable_transport_block_positions", saved = true;
    /// Vanilla `MemoryModuleType.TRANSPORT_ITEMS_COOLDOWN_TICKS`.
    ///
    /// Vanilla registers this one without a codec, so a copper golem reloads
    /// ready to work rather than mid-cooldown.
    TRANSPORT_ITEMS_COOLDOWN_TICKS: i32 = "minecraft:transport_items_cooldown_ticks", saved = false;
    /// Vanilla `MemoryModuleType.RAM_TARGET`.
    RAM_TARGET: DVec3 = "minecraft:ram_target", saved = false;
    /// Vanilla `MemoryModuleType.RAM_COOLDOWN_TICKS`.
    RAM_COOLDOWN_TICKS: i32 = "minecraft:ram_cooldown_ticks", saved = true;
    /// Vanilla `MemoryModuleType.BREED_TARGET`.
    BREED_TARGET: EntityMemory = "minecraft:breed_target", saved = false;
    /// Vanilla `MemoryModuleType.RIDE_TARGET`.
    RIDE_TARGET: EntityMemory = "minecraft:ride_target", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_NEMESIS`.
    NEAREST_VISIBLE_NEMESIS: EntityMemory = "minecraft:nearest_visible_nemesis", saved = false;
    /// Vanilla `MemoryModuleType.ANGRY_AT`.
    ANGRY_AT: Uuid = "minecraft:angry_at", saved = true;
    /// Vanilla `MemoryModuleType.UNIVERSAL_ANGER`.
    UNIVERSAL_ANGER: bool = "minecraft:universal_anger", saved = true;
    /// Vanilla `MemoryModuleType.ADMIRING_ITEM`.
    ADMIRING_ITEM: bool = "minecraft:admiring_item", saved = true;
    /// Vanilla `MemoryModuleType.TIME_TRYING_TO_REACH_ADMIRE_ITEM`.
    TIME_TRYING_TO_REACH_ADMIRE_ITEM: i32 = "minecraft:time_trying_to_reach_admire_item", saved = false;
    /// Vanilla `MemoryModuleType.DISABLE_WALK_TO_ADMIRE_ITEM`.
    DISABLE_WALK_TO_ADMIRE_ITEM: bool = "minecraft:disable_walk_to_admire_item", saved = false;
    /// Vanilla `MemoryModuleType.ADMIRING_DISABLED`.
    ADMIRING_DISABLED: bool = "minecraft:admiring_disabled", saved = true;
    /// Vanilla `MemoryModuleType.HUNTED_RECENTLY`.
    HUNTED_RECENTLY: bool = "minecraft:hunted_recently", saved = true;
    /// Vanilla `MemoryModuleType.CELEBRATE_LOCATION`.
    CELEBRATE_LOCATION: BlockPos = "minecraft:celebrate_location", saved = false;
    /// Vanilla `MemoryModuleType.DANCING`.
    DANCING: bool = "minecraft:dancing", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_HUNTABLE_HOGLIN`.
    NEAREST_VISIBLE_HUNTABLE_HOGLIN: EntityMemory = "minecraft:nearest_visible_huntable_hoglin", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_BABY_HOGLIN`.
    NEAREST_VISIBLE_BABY_HOGLIN: EntityMemory = "minecraft:nearest_visible_baby_hoglin", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD`.
    NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD: EntityMemory = "minecraft:nearest_targetable_player_not_wearing_gold", saved = false;
    /// Vanilla `MemoryModuleType.NEARBY_ADULT_PIGLINS`.
    NEARBY_ADULT_PIGLINS: Vec<EntityMemory> = "minecraft:nearby_adult_piglins", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_ADULT_PIGLINS`.
    NEAREST_VISIBLE_ADULT_PIGLINS: Vec<EntityMemory> = "minecraft:nearest_visible_adult_piglins", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_ADULT_HOGLINS`.
    NEAREST_VISIBLE_ADULT_HOGLINS: Vec<EntityMemory> = "minecraft:nearest_visible_adult_hoglins", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_ADULT_PIGLIN`.
    NEAREST_VISIBLE_ADULT_PIGLIN: EntityMemory = "minecraft:nearest_visible_adult_piglin", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_VISIBLE_ZOMBIFIED`.
    NEAREST_VISIBLE_ZOMBIFIED: EntityMemory = "minecraft:nearest_visible_zombified", saved = false;
    /// Vanilla `MemoryModuleType.VISIBLE_ADULT_PIGLIN_COUNT`.
    VISIBLE_ADULT_PIGLIN_COUNT: i32 = "minecraft:visible_adult_piglin_count", saved = false;
    /// Vanilla `MemoryModuleType.VISIBLE_ADULT_HOGLIN_COUNT`.
    VISIBLE_ADULT_HOGLIN_COUNT: i32 = "minecraft:visible_adult_hoglin_count", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_PLAYER_HOLDING_WANTED_ITEM`.
    NEAREST_PLAYER_HOLDING_WANTED_ITEM: EntityMemory = "minecraft:nearest_player_holding_wanted_item", saved = false;
    /// Vanilla `MemoryModuleType.ATE_RECENTLY`.
    ATE_RECENTLY: bool = "minecraft:ate_recently", saved = false;
    /// Vanilla `MemoryModuleType.NEAREST_REPELLENT`.
    NEAREST_REPELLENT: BlockPos = "minecraft:nearest_repellent", saved = false;
    /// Vanilla `MemoryModuleType.PACIFIED`.
    PACIFIED: bool = "minecraft:pacified", saved = false;
    /// Vanilla `MemoryModuleType.ITEM_PICKUP_COOLDOWN_TICKS`.
    ITEM_PICKUP_COOLDOWN_TICKS: i32 = "minecraft:item_pickup_cooldown_ticks", saved = true;
    /// Vanilla `MemoryModuleType.IS_PREGNANT`, which is what a bred frog carries
    /// until it finds water to lay its spawn on.
    IS_PREGNANT: Unit = "minecraft:is_pregnant", saved = true;
    /// Vanilla `MemoryModuleType.LONG_JUMP_COOLDOWN_TICKS`, registry key
    /// `long_jump_cooling_down`.
    LONG_JUMP_COOLDOWN_TICKS: i32 = "minecraft:long_jump_cooling_down", saved = true;
    /// Vanilla `MemoryModuleType.LONG_JUMP_MID_JUMP`.
    LONG_JUMP_MID_JUMP: bool = "minecraft:long_jump_mid_jump", saved = false;
    /// Vanilla `MemoryModuleType.UNREACHABLE_TONGUE_TARGETS`.
    UNREACHABLE_TONGUE_TARGETS: Vec<Uuid> = "minecraft:unreachable_tongue_targets", saved = false;
    /// Vanilla `MemoryModuleType.PLAY_DEAD_TICKS`.
    PLAY_DEAD_TICKS: i32 = "minecraft:play_dead_ticks", saved = true;
    /// Vanilla `MemoryModuleType.HAS_HUNTING_COOLDOWN`.
    HAS_HUNTING_COOLDOWN: bool = "minecraft:has_hunting_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.DANGER_DETECTED_RECENTLY`, which is both what
    /// keeps an armadillo balled up and, as it expires, what makes it peek.
    DANGER_DETECTED_RECENTLY: bool = "minecraft:danger_detected_recently", saved = true;
    /// Vanilla `MemoryModuleType.LIKED_PLAYER`, the player an allay fetches for.
    LIKED_PLAYER: Uuid = "minecraft:liked_player", saved = true;
    /// Vanilla `MemoryModuleType.LIKED_NOTEBLOCK_POSITION`, registry key
    /// `liked_noteblock`.
    LIKED_NOTEBLOCK_POSITION: GlobalPos = "minecraft:liked_noteblock", saved = true;
    /// Vanilla `MemoryModuleType.LIKED_NOTEBLOCK_COOLDOWN_TICKS`.
    LIKED_NOTEBLOCK_COOLDOWN_TICKS: i32 = "minecraft:liked_noteblock_cooldown_ticks", saved = true;
    /// Vanilla `MemoryModuleType.SNIFF_COOLDOWN`.
    SNIFF_COOLDOWN: Unit = "minecraft:sniff_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.SNIFFER_EXPLORED_POSITIONS`, newest first.
    SNIFFER_EXPLORED_POSITIONS: Vec<GlobalPos> = "minecraft:sniffer_explored_positions", saved = true;
    /// Vanilla `MemoryModuleType.SNIFFER_SNIFFING_TARGET`.
    SNIFFER_SNIFFING_TARGET: BlockPos = "minecraft:sniffer_sniffing_target", saved = false;
    /// Vanilla `MemoryModuleType.SNIFFER_DIGGING`.
    SNIFFER_DIGGING: bool = "minecraft:sniffer_digging", saved = false;
    /// Vanilla `MemoryModuleType.SNIFFER_HAPPY`.
    SNIFFER_HAPPY: bool = "minecraft:sniffer_happy", saved = false;
    /// Vanilla `MemoryModuleType.BREEZE_JUMP_COOLDOWN`, the pause between long
    /// jumps -- two ticks if something has just hurt the breeze, ten otherwise.
    BREEZE_JUMP_COOLDOWN: Unit = "minecraft:breeze_jump_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.BREEZE_SHOOT`, set while the breeze has a
    /// reason to fire and cleared once it has.
    BREEZE_SHOOT: Unit = "minecraft:breeze_shoot", saved = true;
    /// Vanilla `MemoryModuleType.BREEZE_SHOOT_CHARGING`, the inhale before the
    /// wind charge leaves.
    BREEZE_SHOOT_CHARGING: Unit = "minecraft:breeze_shoot_charging", saved = true;
    /// Vanilla `MemoryModuleType.BREEZE_SHOOT_RECOVERING`, registry key
    /// `breeze_shoot_recover`.
    BREEZE_SHOOT_RECOVERING: Unit = "minecraft:breeze_shoot_recover", saved = true;
    /// Vanilla `MemoryModuleType.BREEZE_SHOOT_COOLDOWN`.
    BREEZE_SHOOT_COOLDOWN: Unit = "minecraft:breeze_shoot_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.BREEZE_JUMP_INHALING`, the crouch before a
    /// long jump.
    BREEZE_JUMP_INHALING: Unit = "minecraft:breeze_jump_inhaling", saved = true;
    /// Vanilla `MemoryModuleType.BREEZE_JUMP_TARGET`, the block a breeze has
    /// picked to land on.
    BREEZE_JUMP_TARGET: BlockPos = "minecraft:breeze_jump_target", saved = true;
    /// Vanilla `MemoryModuleType.BREEZE_LEAVING_WATER`, which tells a jump that
    /// started in water not to read the water it is still in as a landing.
    BREEZE_LEAVING_WATER: Unit = "minecraft:breeze_leaving_water", saved = true;
    /// Vanilla `MemoryModuleType.ROAR_TARGET`, who a warden is about to roar at.
    ROAR_TARGET: EntityMemory = "minecraft:roar_target", saved = false;
    /// Vanilla `MemoryModuleType.DISTURBANCE_LOCATION`, where a warden last heard something.
    DISTURBANCE_LOCATION: BlockPos = "minecraft:disturbance_location", saved = false;
    /// Vanilla `MemoryModuleType.RECENT_PROJECTILE`.
    RECENT_PROJECTILE: Unit = "minecraft:recent_projectile", saved = true;
    /// Vanilla `MemoryModuleType.IS_SNIFFING`.
    IS_SNIFFING: Unit = "minecraft:is_sniffing", saved = true;
    /// Vanilla `MemoryModuleType.IS_EMERGING`.
    IS_EMERGING: Unit = "minecraft:is_emerging", saved = true;
    /// Vanilla `MemoryModuleType.ROAR_SOUND_DELAY`.
    ROAR_SOUND_DELAY: Unit = "minecraft:roar_sound_delay", saved = true;
    /// Vanilla `MemoryModuleType.DIG_COOLDOWN`.
    DIG_COOLDOWN: Unit = "minecraft:dig_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.ROAR_SOUND_COOLDOWN`.
    ROAR_SOUND_COOLDOWN: Unit = "minecraft:roar_sound_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.TOUCH_COOLDOWN`.
    TOUCH_COOLDOWN: Unit = "minecraft:touch_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.VIBRATION_COOLDOWN`.
    VIBRATION_COOLDOWN: Unit = "minecraft:vibration_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.SONIC_BOOM_COOLDOWN`.
    SONIC_BOOM_COOLDOWN: Unit = "minecraft:sonic_boom_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.SONIC_BOOM_SOUND_COOLDOWN`.
    SONIC_BOOM_SOUND_COOLDOWN: Unit = "minecraft:sonic_boom_sound_cooldown", saved = true;
    /// Vanilla `MemoryModuleType.SONIC_BOOM_SOUND_DELAY`.
    SONIC_BOOM_SOUND_DELAY: Unit = "minecraft:sonic_boom_sound_delay", saved = true;
}

/// Everything a brain remembers.
///
/// Vanilla parity: the `memories` field of `Brain`, plus the `registerMemory`
/// that seeds it. A memory that was never registered is invisible to
/// [`Self::check`] -- that is what makes a behavior requiring
/// [`MemoryStatus::Registered`] refuse to start on a mob whose brain does not
/// know that memory at all.
#[derive(Debug, Default)]
pub(super) struct Memories {
    slots: FxHashMap<MemoryModuleId, MemorySlot>,
}

impl Memories {
    /// Vanilla parity: `Brain.registerMemory`.
    pub(super) fn register(&mut self, id: MemoryModuleId, serializable: bool) {
        self.slots
            .entry(id)
            .or_insert_with(|| MemorySlot::empty(serializable));
    }

    /// Vanilla parity: `Brain.forgetOutdatedMemories`.
    pub(super) fn tick(&mut self) {
        for slot in self.slots.values_mut() {
            slot.tick();
        }
    }

    /// Vanilla parity: `Brain.checkMemory`.
    pub(super) fn check(&self, id: MemoryModuleId, status: MemoryStatus) -> bool {
        let Some(slot) = self.slots.get(&id) else {
            return false;
        };
        match status {
            MemoryStatus::Registered => true,
            MemoryStatus::ValuePresent => slot.has_value(),
            MemoryStatus::ValueAbsent => !slot.has_value(),
        }
    }

    /// Vanilla parity: `Brain.getMemory`, minus the throw for an unregistered
    /// memory -- Steel answers `None` for both "not registered" and "empty",
    /// because a behavior that reads a memory it never declared is a bug the
    /// required-memory list already prevents.
    pub(super) fn get(&self, id: MemoryModuleId) -> Option<&MemoryValue> {
        self.slots.get(&id)?.value.as_ref()
    }

    /// Vanilla parity: `Brain.getTimeUntilExpiry`.
    pub(super) fn time_to_live(&self, id: MemoryModuleId) -> i64 {
        self.slots
            .get(&id)
            .map_or(NEVER_EXPIRE, |slot| slot.time_to_live)
    }

    /// Vanilla parity: the private `Brain.setMemoryInternal` pair. Setting an
    /// empty collection erases the memory, exactly as vanilla's
    /// `isEmptyCollection` check does.
    pub(super) fn set(
        &mut self,
        id: MemoryModuleId,
        value: Option<MemoryValue>,
        time_to_live: i64,
    ) {
        let Some(slot) = self.slots.get_mut(&id) else {
            return;
        };
        match value {
            Some(value) if !value.is_empty_collection() => {
                slot.value = Some(value);
                slot.time_to_live = time_to_live;
            }
            _ => slot.clear(),
        }
    }

    /// Vanilla parity: `Brain.eraseMemory`.
    pub(super) fn erase(&mut self, id: MemoryModuleId) {
        if let Some(slot) = self.slots.get_mut(&id) {
            slot.clear();
        }
    }

    /// Vanilla parity: `Brain.clearMemories`.
    pub(super) fn clear(&mut self) {
        for slot in self.slots.values_mut() {
            slot.clear();
        }
    }

    /// Vanilla parity: `Brain.isBrainDead`, memory half.
    pub(super) fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Writes every serializable memory the way `Brain.Packed.CODEC` would.
    ///
    /// Vanilla parity: `Brain.pack` feeding `MemoryMap.CODEC`, which writes a
    /// `{ "<registry key>": { "value": ..., "ttl": ... } }` map.
    pub(super) fn pack(&self) -> NbtCompound {
        let mut memories = NbtCompound::new();
        for (id, slot) in &self.slots {
            if !slot.serializable {
                continue;
            }
            let Some(value) = slot.value.as_ref().and_then(MemoryValue::to_nbt) else {
                continue;
            };
            let mut entry = NbtCompound::new();
            entry.insert("value", value);
            if slot.time_to_live != NEVER_EXPIRE {
                entry.insert("ttl", slot.time_to_live);
            }
            memories.insert(id.key(), NbtTag::Compound(entry));
        }
        memories
    }

    /// Reads back what [`Self::pack`] wrote.
    ///
    /// Vanilla parity: the `for (MemoryMap.Value<?> memory : memories)` loop of
    /// the `Brain` constructor. A saved memory the mob's own brain never
    /// registered is dropped, because vanilla's `setMemoryInternal` no-ops on a
    /// missing slot. Unknown keys are skipped for the same reason, which is
    /// what lets a world written by a newer build still load.
    pub(super) fn restore(&mut self, memories: &BorrowedNbtCompound<'_, '_>) {
        for &(key, saved, reader) in MEMORY_TABLE {
            if !saved {
                continue;
            }
            let id = MemoryModuleId(key);
            if !self.slots.contains_key(&id) {
                continue;
            }
            let Some(entry) = memories.compound(key) else {
                continue;
            };
            let Some(value) = entry.get("value").as_ref().and_then(|tag| reader(tag)) else {
                continue;
            };
            self.set(id, Some(value), entry.long("ttl").unwrap_or(NEVER_EXPIRE));
        }
    }
}
