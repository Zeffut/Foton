//! The mobs a raid is made of.
//!
//! Vanilla parity: `Raider`. Six mobs share this layer -- the four illagers,
//! the ravager and the witch -- and it is what turns a wandering hostile into
//! a member of an organized attack: the wave it belongs to, whether it may be
//! recruited, the captain's banner, the celebration it breaks into when the
//! village falls.
//!
//! A raider carries the id of the raid it belongs to rather than a reference to
//! it: the raid lives in [`crate::raid::Raids`], which the world owns, and a
//! strong reference from mob to raid would be a cycle through the world.
//! [`Raider::current_raid`] resolves the id, and
//! [`Raider::current_raid_status`] is the cheap read the goals branch on --
//! three atomics behind one short map lock, so a mob can ask what its raid is
//! doing from inside its own tick.

use foton_protocol::packets::game::CTakeItemEntity;
use foton_registry::data_components::vanilla_components::{
    BANNER_PATTERNS, ITEM_NAME, RARITY, Rarity, TOOLTIP_DISPLAY, TooltipDisplay,
};
use foton_registry::data_components::{BannerPatternLayer, BannerPatternLayers};
use foton_registry::item_stack::ItemStack;
use foton_registry::registry::holder::RegistryHolder;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::{DyeColor, vanilla_banner_patterns, vanilla_entities, vanilla_items};
use foton_utils::ChunkPos;
use foton_utils::Downcast as _;
use foton_utils::locks::SyncMutex;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use std::borrow::Cow;
use std::sync::Arc;
use text_components::{TextComponent, translation::TranslatedMessage};

use crate::entity::damage::DamageSource;
use crate::entity::entities::ItemEntity;
use crate::entity::patrolling_monster::{PATROL_LEADER_SPAWN_CHANCE, PatrollingMonster};
use crate::entity::{EntitySpawnReason, LivingEntity, RemovalReason, SharedEntity};
use crate::inventory::equipment::EquipmentSlot;
use crate::raid::Raid;
use crate::world::World;

/// NBT key vanilla stores the raid wave under.
pub const TAG_WAVE: &str = "Wave";
/// NBT key vanilla stores the recruitable flag under.
pub const TAG_CAN_JOIN_RAID: &str = "CanJoinRaid";
/// NBT key vanilla stores the id of the raid this mob belongs to under.
pub const TAG_RAID_ID: &str = "RaidId";

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
/// over. The three answers are bundled here because they are the whole of what
/// a goal needs and they are the only part of a raid that can be read from
/// inside a mob's own tick without touching the raid's state lock.
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
/// Vanilla keeps these on the mob; Foton groups them so an entity holds one
/// field, the way it holds a [`crate::entity::MobBase`].
#[derive(Debug)]
pub struct RaiderState {
    /// Which wave of the raid this mob arrived with.
    wave: SyncMutex<i32>,
    /// Whether a passing raid is allowed to recruit this mob.
    can_join_raid: SyncMutex<bool>,
    /// Ticks this mob has spent away from the raid it belongs to.
    ticks_outside_raid: SyncMutex<i32>,
    /// The raid this mob belongs to, by its key in [`crate::raid::Raids`].
    ///
    /// Vanilla parity: `Raider.raid`, which is the raid object itself.
    raid_id: SyncMutex<Option<i32>>,
}

impl RaiderState {
    /// Creates the membership of a mob that belongs to no raid.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            wave: SyncMutex::new(0),
            can_join_raid: SyncMutex::new(false),
            ticks_outside_raid: SyncMutex::new(0),
            raid_id: SyncMutex::new(None),
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
    /// Vanilla parity: `applyRaidBuffs`, called from `Raid.joinRaid` as each
    /// mob is dropped into its wave. Every vanilla buff is an enchantment
    /// provider, which Foton does not have, so what lands is the unenchanted
    /// half: the vindicator's iron axe, and nothing for anybody else.
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

    /// Returns the raid this mob belongs to.
    ///
    /// Vanilla parity: `getCurrentRaid`.
    fn current_raid(&self) -> Option<Arc<Raid>> {
        let raid_id = (*self.raider_state().raid_id.lock())?;
        self.level()?.raids().get(raid_id)
    }

    /// Puts this mob in a raid, or takes it out of one.
    ///
    /// Vanilla parity: `setCurrentRaid`.
    fn set_current_raid(&self, raid_id: Option<i32>) {
        *self.raider_state().raid_id.lock() = raid_id;
    }

    /// Returns what the raid this mob belongs to is doing.
    ///
    /// Vanilla parity: `getCurrentRaid`, collapsed to the three flags its
    /// callers actually read.
    fn current_raid_status(&self) -> Option<RaidStatus> {
        self.current_raid().map(|raid| raid.status())
    }

    /// Returns whether this mob is in a raid, or standing in one.
    ///
    /// Vanilla parity: `Raider.hasRaid`.
    fn has_raid(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        self.current_raid().is_some() || world.is_raided(self.block_position())
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
/// on a `Raid` because the patrol captain wears one and Foton has patrols
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

    // Vanilla parity: `Raider.finalizeSpawn`, which holds a naturally spawned
    // witch out of raids -- a swamp hut witch is not a raider looking for a
    // village -- and lets every other raider be recruited.
    raider.set_can_join_raid(
        raider.entity_type() != &vanilla_entities::WITCH
            || spawn_reason != EntitySpawnReason::Natural,
    );
}

/// Writes the raid membership the way vanilla does.
///
/// Vanilla parity: `Raider.addAdditionalSaveData`.
pub fn write_raider_state(mob: &dyn Raider, nbt: &mut NbtCompound) {
    nbt.insert(TAG_WAVE, mob.wave());
    nbt.insert(TAG_CAN_JOIN_RAID, i8::from(mob.can_join_raid()));
    if let Some(raid_id) = *mob.raider_state().raid_id.lock() {
        nbt.insert(TAG_RAID_ID, raid_id);
    }
}

/// Reads the raid membership the way vanilla does.
///
/// Vanilla parity: `Raider.readAdditionalSaveData`, including the way a mob
/// puts itself back into its wave: a raid does not persist its raiders, so a
/// reloaded one has to re-register or the wave would look empty and the next
/// one would spawn on top of it.
pub fn read_raider_state(mob: &dyn Raider, nbt: BorrowedNbtCompoundView<'_, '_>) {
    mob.set_wave(nbt.int(TAG_WAVE).unwrap_or(0));
    mob.set_can_join_raid(nbt.byte(TAG_CAN_JOIN_RAID).is_some_and(|value| value != 0));

    let Some(raid_id) = nbt.int(TAG_RAID_ID) else {
        return;
    };
    let Some(world) = mob.level() else {
        return;
    };
    let Some(raid) = world.raids().get(raid_id) else {
        return;
    };
    mob.set_current_raid(Some(raid_id));
    raid.add_wave_mob(&world, mob.wave(), mob, false);
    if mob.is_patrol_leader() {
        raid.set_leader(mob.wave(), mob);
    }
}

/// Runs the raid half of a raider's tick.
///
/// Vanilla parity: `Raider.aiStep`. A raider that wandered into a raid it does
/// not belong to is recruited by it, once a second; a raider already in one is
/// kept from counting as idle while it has a player or a golem to fight.
pub fn ai_step_raider(mob: &dyn Raider) {
    let Some(world) = mob.level() else {
        return;
    };
    if !LivingEntity::is_alive(mob) || !mob.can_join_raid() {
        return;
    }

    let Some(raid) = mob.current_raid() else {
        if world.game_time() % 20 != 0 {
            return;
        }
        let Some(nearby_raid) = world.get_raid_at(mob.block_position()) else {
            return;
        };
        if mob.is_recruitable() {
            nearby_raid.join_raid(&world, nearby_raid.groups_spawned(), mob, None, true);
        }
        return;
    };
    drop(raid);

    let Some(target) = mob.target() else {
        return;
    };
    let target_type = target.entity_type();
    if target_type == &vanilla_entities::PLAYER || target_type == &vanilla_entities::IRON_GOLEM {
        mob.set_no_action_time(0);
    }
}

/// Takes a dying raider out of its raid.
///
/// Vanilla parity: the raid half of `Raider.die`, which runs before the shared
/// body. The killer becoming a Hero of the Village is decided here, on every
/// raider's death, and only paid out if the village wins.
pub fn die_raider(mob: &dyn Raider, source: &DamageSource) {
    let Some(world) = mob.level() else {
        return;
    };
    let Some(raid) = mob.current_raid() else {
        return;
    };

    if mob.is_patrol_leader() {
        raid.remove_leader(mob.wave());
    }
    if let Some(killer_id) = source.causing_entity_id
        && let Some(killer) = world.get_entity_by_id(killer_id)
        && killer.entity_type() == &vanilla_entities::PLAYER
    {
        raid.add_hero_of_the_village(killer.uuid());
    }
    raid.remove_from_raid(&world, mob.id(), false);
}

/// Redraws the raid bar when one of its mobs is hurt.
///
/// Vanilla parity: the `hasActiveRaid()` guard of `Raider.hurtServer`.
pub fn hurt_server_raider(mob: &dyn Raider) {
    let Some(world) = mob.level() else {
        return;
    };
    let Some(raid) = mob.current_raid() else {
        return;
    };
    if raid.is_active() {
        raid.update_boss_bar(&world);
    }
}

/// Lets a raider take the wave's banner off the ground and become its captain.
///
/// Vanilla parity: `Raider.pickUpItem`. Returns whether the banner was taken,
/// so the caller can fall through to whatever else that mob picks up.
pub fn pick_up_banner(mob: &dyn Raider, world: &Arc<World>, item_entity: &SharedEntity) -> bool {
    let Some(raid) = mob.current_raid() else {
        return false;
    };
    if !raid.is_active() || raid.leader(mob.wave()).is_some() {
        return false;
    }
    let Some(item) = item_entity.downcast_ref::<ItemEntity>() else {
        return false;
    };
    let banner = item.get_item();
    if !is_ominous_banner(&banner) {
        return false;
    }

    let slot = EquipmentSlot::Head;
    let mut current = ItemStack::empty();
    mob.with_equipment_slot(slot, &mut |stack| {
        current = stack.copy_with_count(stack.count());
    });
    if !current.is_empty() && (rand::random::<f32>() - 0.1).max(0.0) < mob.drop_chance(slot) {
        mob.spawn_at_location(current, 0.0);
    }

    let count = banner.count();
    mob.living_base().equipment().lock().set(slot, banner);
    world.broadcast_to_nearby(
        ChunkPos::from_entity_pos(item_entity.position()),
        CTakeItemEntity::new(item_entity.id(), mob.id(), count),
        None,
    );
    item_entity.set_removed(RemovalReason::Discarded);
    raid.set_leader(mob.wave(), mob);
    mob.set_patrol_leader(true);
    true
}
