//! The mobs a raid is made of.
//!
//! Vanilla parity: `Raider`. Six mobs share this layer -- the four illagers,
//! the ravager and the witch -- and it is what turns a wandering hostile into
//! a member of an organized attack: the wave it belongs to, whether it may be
//! recruited, the captain's banner, the celebration it breaks into when the
//! village falls.
//!
//! **Steel has no raid.** `Raid` and `Raids` stand on villagers, an occupied
//! village point-of-interest index, a saved-data manager and a boss bar. The
//! boss bar is now here -- vanilla's raid bar is a plain
//! [`ServerBossEvent`](crate::boss_event::ServerBossEvent), which is what
//! `crate::boss_event` provides -- and the other three are still missing.
//! Every member of this trait that vanilla
//! answers from a live `Raid` therefore answers from nothing here and is marked
//! as such; the raid-independent half -- the patrol captaincy, the banner, the
//! two-per-tick idle clock, the celebration flag and the per-mob raid buffs --
//! is real. Landing `Raid` later means giving [`Raider::current_raid_status`]
//! something to read and nothing else in this file has to move.

use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use std::borrow::Cow;
use steel_registry::data_components::vanilla_components::{
    BANNER_PATTERNS, ITEM_NAME, RARITY, Rarity, TOOLTIP_DISPLAY, TooltipDisplay,
};
use steel_registry::data_components::{BannerPatternLayer, BannerPatternLayers};
use steel_registry::item_stack::ItemStack;
use steel_registry::registry::holder::RegistryHolder;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::{DyeColor, vanilla_banner_patterns, vanilla_items};
use steel_utils::locks::SyncMutex;
use text_components::{TextComponent, translation::TranslatedMessage};

use crate::entity::patrolling_monster::{PATROL_LEADER_SPAWN_CHANCE, PatrollingMonster};
use crate::entity::{EntitySpawnReason, LivingEntity};
use crate::inventory::equipment::EquipmentSlot;

/// NBT key vanilla stores the raid wave under.
pub const TAG_WAVE: &str = "Wave";
/// NBT key vanilla stores the recruitable flag under.
pub const TAG_CAN_JOIN_RAID: &str = "CanJoinRaid";

/// Drop chance vanilla gives the captain's banner.
///
/// Vanilla parity: the `setDropChance(HEAD, 2.0F)` of both
/// `PatrollingMonster.finalizeSpawn` and `Raid.setLeader`. A chance above one
/// is vanilla's way of saying the banner always drops and never takes damage.
pub const OMINOUS_BANNER_DROP_CHANCE: f32 = 2.0;

/// Ticks of doing nothing after which a raider stops counting as recruitable.
///
/// Vanilla parity: `Raid.MAX_NO_ACTION_TIME`, read by `Raids.canJoinRaid`.
pub const MAX_NO_ACTION_TIME: i32 = 2400;

/// What a raid the mob belongs to is currently doing.
///
/// Vanilla reads `getCurrentRaid()` and then asks the raid three questions:
/// whether it is still running, whether the village lost, and whether it is
/// over. Steel has no raid to ask, so the three answers are bundled here and
/// [`Raider::current_raid_status`] returns `None` for every mob. The type
/// exists so the goals that branch on it are written the vanilla way rather
/// than around a hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaidStatus {
    /// Vanilla parity: `Raid.isActive`.
    pub active: bool,
    /// Vanilla parity: `Raid.isLoss`, which is what the raiders celebrate.
    pub loss: bool,
    /// Vanilla parity: `Raid.isOver`.
    pub over: bool,
}

/// The raid membership a raider carries.
///
/// Vanilla keeps these on the mob; Steel groups them so an entity holds one
/// field, the way it holds a [`crate::entity::MobBase`].
#[derive(Debug)]
pub struct RaiderState {
    /// Which wave of the raid this mob arrived with.
    wave: SyncMutex<i32>,
    /// Whether a passing raid is allowed to recruit this mob.
    can_join_raid: SyncMutex<bool>,
    /// Ticks this mob has spent away from the raid it belongs to.
    ticks_outside_raid: SyncMutex<i32>,
}

impl RaiderState {
    /// Creates the membership of a mob that belongs to no raid.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            wave: SyncMutex::new(0),
            can_join_raid: SyncMutex::new(false),
            ticks_outside_raid: SyncMutex::new(0),
        }
    }
}

impl Default for RaiderState {
    fn default() -> Self {
        Self::new()
    }
}

/// A mob a raid is made of.
///
/// Vanilla parity: the `Raider` class.
pub trait Raider: PatrollingMonster {
    /// Returns this mob's raid membership.
    fn raider_state(&self) -> &RaiderState;

    /// Upgrades this mob's gear for the wave it arrived with.
    ///
    /// Vanilla parity: `applyRaidBuffs`. Steel never calls it -- nothing spawns
    /// a wave -- but each mob implements it, so the day a raid manager exists
    /// the pillagers arrive with enchanted crossbows without any of these mobs
    /// being touched.
    fn apply_raid_buffs(&self, wave: i32, is_captain: bool);

    /// Returns the sound this mob makes over a fallen village.
    ///
    /// Vanilla parity: `getCelebrateSound`.
    fn celebrate_sound(&self) -> SoundEventRef;

    /// Returns whether this mob is visibly celebrating.
    ///
    /// Vanilla parity: the `IS_CELEBRATING` synced value, which every raider
    /// carries at data index 16.
    fn is_celebrating(&self) -> bool;

    /// Sets whether this mob is visibly celebrating.
    fn set_celebrating(&self, celebrating: bool);

    /// Returns what the raid this mob belongs to is doing.
    ///
    /// Vanilla parity: `getCurrentRaid`, collapsed to the three flags its
    /// callers actually read. Always `None` in Steel: see the module comment.
    fn current_raid_status(&self) -> Option<RaidStatus> {
        None
    }

    /// Returns whether this mob belongs to a raid that is still running.
    ///
    /// Vanilla parity: `hasActiveRaid`.
    fn has_active_raid(&self) -> bool {
        self.current_raid_status()
            .is_some_and(|status| status.active)
    }

    /// Returns whether a raid is allowed to recruit this mob.
    ///
    /// Vanilla parity: `canJoinRaid`.
    fn can_join_raid(&self) -> bool {
        *self.raider_state().can_join_raid.lock()
    }

    /// Sets whether a raid is allowed to recruit this mob.
    fn set_can_join_raid(&self, can_join_raid: bool) {
        *self.raider_state().can_join_raid.lock() = can_join_raid;
    }

    /// Returns which wave this mob arrived with.
    fn wave(&self) -> i32 {
        *self.raider_state().wave.lock()
    }

    /// Sets which wave this mob arrived with.
    fn set_wave(&self, wave: i32) {
        *self.raider_state().wave.lock() = wave;
    }

    /// Returns how long this mob has been away from its raid.
    ///
    /// Vanilla parity: `getTicksOutsideRaid`.
    fn ticks_outside_raid(&self) -> i32 {
        *self.raider_state().ticks_outside_raid.lock()
    }

    /// Sets how long this mob has been away from its raid.
    fn set_ticks_outside_raid(&self, ticks: i32) {
        *self.raider_state().ticks_outside_raid.lock() = ticks;
    }

    /// Returns whether this mob leads a wave.
    ///
    /// Vanilla parity: `isCaptain`. Both halves have to hold: the banner is
    /// what a player sees, and the patrol leadership is what the raid tracks.
    fn is_captain(&self) -> bool {
        if !self.is_patrol_leader() {
            return false;
        }
        let mut wearing_banner = false;
        self.with_equipment_slot(EquipmentSlot::Head, &mut |item| {
            wearing_banner = is_ominous_banner(item);
        });
        wearing_banner
    }

    /// Returns whether a passing patrol may sweep this mob up.
    ///
    /// Vanilla parity: `Raider.canJoinPatrol`, which refuses once the mob has
    /// a raid to fight in.
    fn can_join_patrol_raider(&self) -> bool {
        !self.has_active_raid()
    }

    /// Returns whether this mob may still be recruited by a nearby raid.
    ///
    /// Vanilla parity: `Raids.canJoinRaid`. A raider that has been standing
    /// idle for two minutes has been forgotten by the game and is not worth
    /// pulling into a wave.
    fn is_recruitable(&self) -> bool {
        LivingEntity::is_alive(self)
            && self.can_join_raid()
            && self.no_action_time() <= MAX_NO_ACTION_TIME
    }

    /// Returns vanilla `Raider.removeWhenFarAway`.
    fn remove_when_far_away_raider(&self, dist_sqr: f64) -> bool {
        self.current_raid_status().is_none() && self.remove_when_far_away_patrolling(dist_sqr)
    }

    /// Returns vanilla `Raider.requiresCustomPersistence`.
    fn requires_custom_persistence_raider(&self) -> bool {
        self.current_raid_status().is_some()
    }
}

/// Returns whether `item` is the banner a raid captain wears.
///
/// Vanilla parity: the `ItemStack.matches(banner, Raid.getOminousBannerInstance(..))`
/// of `Raider.isCaptain`, narrowed to the pattern list because that is the part
/// that identifies the banner.
#[must_use]
pub fn is_ominous_banner(item: &ItemStack) -> bool {
    item.is(&vanilla_items::WHITE_BANNER)
        && item
            .get(BANNER_PATTERNS)
            .is_some_and(|patterns| patterns.layers() == ominous_banner_layers().layers())
}

/// Builds the ominous banner.
///
/// Vanilla parity: `Raid.getOminousBannerInstance`. It lives here rather than
/// on a `Raid` because the patrol captain wears one and Steel has patrols
/// without raids.
#[must_use]
pub fn ominous_banner() -> ItemStack {
    let mut banner = ItemStack::new(&vanilla_items::WHITE_BANNER);
    banner.set(BANNER_PATTERNS, ominous_banner_layers());
    banner.set(
        TOOLTIP_DISPLAY,
        TooltipDisplay::DEFAULT.with_hidden(BANNER_PATTERNS, true),
    );
    banner.set(
        ITEM_NAME,
        TextComponent::translated(TranslatedMessage {
            key: Cow::Borrowed("block.minecraft.ominous_banner"),
            fallback: None,
            args: None,
        }),
    );
    banner.set(RARITY, Rarity::Uncommon);
    banner
}

/// Returns the eight layers of the ominous banner, in vanilla's order.
///
/// Vanilla parity: `Raid.getBannerComponentPatch`. `RHOMBUS_MIDDLE` and
/// `CIRCLE_MIDDLE` are the Java constant names for the registry entries
/// `rhombus` and `circle`.
fn ominous_banner_layers() -> BannerPatternLayers {
    BannerPatternLayers::new(vec![
        BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::RHOMBUS),
            DyeColor::Cyan,
        ),
        BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::STRIPE_BOTTOM),
            DyeColor::LightGray,
        ),
        BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::STRIPE_CENTER),
            DyeColor::Gray,
        ),
        BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::BORDER),
            DyeColor::LightGray,
        ),
        BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::STRIPE_MIDDLE),
            DyeColor::Black,
        ),
        BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::HALF_HORIZONTAL),
            DyeColor::LightGray,
        ),
        BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::CIRCLE),
            DyeColor::LightGray,
        ),
        BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::BORDER),
            DyeColor::Black,
        ),
    ])
}

/// Runs the spawn work every raider shares.
///
/// Vanilla parity: `PatrollingMonster.finalizeSpawn` followed by
/// `Raider.finalizeSpawn`. Six mobs run both, so it lives here rather than
/// being copied into each of them.
pub fn finalize_spawn_raider(raider: &dyn Raider, spawn_reason: EntitySpawnReason) {
    let picked_as_captain = !matches!(
        spawn_reason,
        EntitySpawnReason::Patrol | EntitySpawnReason::Event | EntitySpawnReason::Structure
    ) && rand::random::<f32>() < PATROL_LEADER_SPAWN_CHANCE
        && raider.can_be_leader();
    if picked_as_captain {
        raider.set_patrol_leader(true);
    }

    if raider.is_patrol_leader() {
        raider
            .living_base()
            .equipment()
            .lock()
            .set(EquipmentSlot::Head, ominous_banner());
        raider.set_drop_chance(EquipmentSlot::Head, OMINOUS_BANNER_DROP_CHANCE);
    }

    if spawn_reason == EntitySpawnReason::Patrol {
        raider.set_patrolling(true);
    }

    // Vanilla parity: `Raider.finalizeSpawn`, which only holds a naturally
    // spawned witch out of raids. Steel's witch is not a raider yet, so every
    // raider that spawns here is recruitable.
    raider.set_can_join_raid(true);
}

/// Writes the raid membership the way vanilla does.
///
/// Vanilla parity: `Raider.addAdditionalSaveData`, minus the `RaidId` it writes
/// for a live raid.
pub fn write_raider_state(mob: &dyn Raider, nbt: &mut NbtCompound) {
    nbt.insert(TAG_WAVE, mob.wave());
    nbt.insert(TAG_CAN_JOIN_RAID, i8::from(mob.can_join_raid()));
}

/// Reads the raid membership the way vanilla does.
///
/// Vanilla parity: `Raider.readAdditionalSaveData`, minus the `RaidId` lookup.
pub fn read_raider_state(mob: &dyn Raider, nbt: BorrowedNbtCompoundView<'_, '_>) {
    mob.set_wave(nbt.int(TAG_WAVE).unwrap_or(0));
    mob.set_can_join_raid(nbt.byte(TAG_CAN_JOIN_RAID).is_some_and(|value| value != 0));
}
