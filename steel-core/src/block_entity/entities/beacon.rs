//! Beacon block entity.
//!
//! Vanilla parity: `BeaconBlockEntity`. A beacon counts the solid pyramid
//! under it, checks that nothing opaque stands between it and the sky, and
//! every four seconds hands its two chosen effects to every player within
//! range.
//!
//! Not implemented: the colored beam. Vanilla walks the column above the
//! beacon collecting the stained glass it passes through, and sends the
//! resulting sections to the client to draw. Steel keeps only the part of that
//! walk the server needs -- whether the column is clear at all, which is what
//! decides if the beacon works.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_registry::mob_effect::MobEffectRef;
use steel_registry::{
    REGISTRY, RegistryEntry, RegistryExt as _, TaggedRegistryExt as _, vanilla_block_entity_types,
    vanilla_block_tags::BlockTag,
};

use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, Identifier};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;

use crate::entity::{Entity as _, LivingEntity as _, MobEffectInstance};
use crate::player::Player;
use crate::world::World;

/// The tallest pyramid a beacon reads.
///
/// Vanilla parity: `BeaconBlockEntity.MAX_LEVELS`.
pub const MAX_LEVELS: i32 = 4;

/// The pyramid a secondary effect needs.
///
/// Vanilla parity: `BeaconBlockEntity.LEVELS_NEEDED_FOR_SECONDARY`.
pub const LEVELS_NEEDED_FOR_SECONDARY: i32 = 4;

/// Values the beacon menu mirrors to the client: levels, primary, secondary.
///
/// Vanilla parity: `BeaconBlockEntity.NUM_DATA_VALUES`.
pub const BEACON_DATA_SLOTS: usize = 3;

/// The light dampening of a block nothing sees through.
///
/// Vanilla parity: the `getLightDampening() >= 15` of the beam walk.
const FULL_LIGHT_DAMPENING: u8 = 15;

/// How often the pyramid is recounted and the effects handed out.
///
/// Vanilla parity: the `gameTime % 80` of `BeaconBlockEntity.tick`.
const APPLY_INTERVAL_TICKS: u64 = 80;

/// Effects a beacon offers, by the pyramid level each needs.
///
/// Vanilla parity: `BeaconBlockEntity.BEACON_EFFECTS`.
const fn beacon_effects() -> [&'static [&'static str]; 4] {
    [
        &["speed", "haste"],
        &["resistance", "jump_boost"],
        &["strength"],
        &["regeneration"],
    ]
}

/// Returns the pyramid level `effect` needs, or `None` if a beacon never
/// offers it.
///
/// Vanilla parity: `BeaconBlockEntity.getRequiredLevelsFor`, which answers
/// `Integer.MAX_VALUE` for an effect that is not on the list; `None` says the
/// same thing without a sentinel.
#[must_use]
pub fn required_levels_for(effect: MobEffectRef) -> Option<i32> {
    let path = effect.key().path.as_ref();
    beacon_effects()
        .iter()
        .position(|names| names.contains(&path))
        .and_then(|index| i32::try_from(index + 1).ok())
}

/// Returns whether this pair of effects can be set on a beacon of `levels`.
///
/// Vanilla parity: `BeaconBlockEntity.validateEffects`. The rule reads oddly
/// until you see what it is for: the secondary slot only ever holds
/// regeneration, or a second helping of the primary.
#[must_use]
pub fn validate_effects(
    primary: Option<MobEffectRef>,
    secondary: Option<MobEffectRef>,
    levels: i32,
) -> bool {
    if secondary.is_some() && levels < LEVELS_NEEDED_FOR_SECONDARY {
        return false;
    }
    let Some(primary_level) = level_or_zero(primary) else {
        return false;
    };
    let Some(secondary_level) = level_or_zero(secondary) else {
        return false;
    };
    if primary_level > levels || secondary_level > levels {
        return false;
    }
    if primary_level >= LEVELS_NEEDED_FOR_SECONDARY {
        return false;
    }
    secondary_level == 0
        || secondary_level >= LEVELS_NEEDED_FOR_SECONDARY
        || primary
            .zip(secondary)
            .is_some_and(|(a, b)| a.key() == b.key())
}

/// The level an effect needs, treating "no effect" as zero and an effect no
/// beacon offers as a failure.
fn level_or_zero(effect: Option<MobEffectRef>) -> Option<i32> {
    match effect {
        None => Some(0),
        Some(effect) => required_levels_for(effect),
    }
}

/// Turns a mob effect into the number the beacon protocol carries.
///
/// Vanilla parity: `BeaconMenu.encodeEffect`, which offsets by one so that
/// zero can mean "none".
#[must_use]
pub fn encode_effect(effect: Option<MobEffectRef>) -> i32 {
    effect
        .and_then(RegistryEntry::try_id)
        .and_then(|id| i32::try_from(id + 1).ok())
        .unwrap_or(0)
}

/// Reads back what [`encode_effect`] wrote.
///
/// Vanilla parity: `BeaconMenu.decodeEffect`.
///
/// This is the *data slot* encoding, offset by one. The `SetBeacon` packet
/// carries a plain registry id instead -- see [`effect_from_holder_id`]. The
/// two differ by one, so mixing them up hands out the neighboring effect
/// rather than failing.
#[must_use]
pub fn decode_effect(id: i32) -> Option<MobEffectRef> {
    if id <= 0 {
        return None;
    }
    let index = usize::try_from(id - 1).ok()?;
    REGISTRY.mob_effects.by_id(index)
}

/// Reads a mob effect out of a `Holder<MobEffect>` sent by the client.
///
/// Vanilla parity: the holder decoding of `ServerboundSetBeaconPacket`, which
/// is a plain registry id with no offset -- unlike the data slots the same
/// menu publishes.
#[must_use]
pub fn effect_from_holder_id(id: i32) -> Option<MobEffectRef> {
    let index = usize::try_from(id).ok()?;
    REGISTRY.mob_effects.by_id(index)
}

/// Keeps only an effect a beacon actually offers.
///
/// Vanilla parity: `BeaconBlockEntity.filterEffect`, which is what stops a
/// crafted packet handing a player night vision from a beacon.
fn filter_effect(effect: Option<MobEffectRef>) -> Option<MobEffectRef> {
    effect.filter(|effect| required_levels_for(effect).is_some())
}

/// Beacon block entity.
pub struct BeaconBlockEntity {
    base: Arc<BlockEntityBase>,
    data: Arc<BeaconDataSlots>,
}

/// The three values the beacon menu mirrors to the client.
pub struct BeaconDataSlots {
    levels: AtomicI32,
    primary: SyncMutex<Option<MobEffectRef>>,
    secondary: SyncMutex<Option<MobEffectRef>>,
}

impl Default for BeaconDataSlots {
    fn default() -> Self {
        Self {
            levels: AtomicI32::new(0),
            primary: SyncMutex::new(None),
            secondary: SyncMutex::new(None),
        }
    }
}

impl BeaconDataSlots {
    /// How many rings of the pyramid are complete.
    #[must_use]
    pub fn levels(&self) -> i32 {
        self.levels.load(Ordering::Relaxed)
    }

    /// The effect handed to everyone in range.
    #[must_use]
    pub fn primary(&self) -> Option<MobEffectRef> {
        *self.primary.lock()
    }

    /// The second effect, which a full pyramid unlocks.
    #[must_use]
    pub fn secondary(&self) -> Option<MobEffectRef> {
        *self.secondary.lock()
    }

    /// Reads the three values in the order the vanilla protocol expects.
    #[must_use]
    pub fn snapshot(&self) -> [i16; BEACON_DATA_SLOTS] {
        [
            i16::try_from(self.levels()).unwrap_or(i16::MAX),
            i16::try_from(encode_effect(self.primary())).unwrap_or(0),
            i16::try_from(encode_effect(self.secondary())).unwrap_or(0),
        ]
    }
}

// SAFETY: This key is owned by Steel and uniquely identifies `BeaconBlockEntity`.
unsafe impl DowncastType for BeaconBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/beacon");
}

impl BeaconBlockEntity {
    /// Creates a beacon block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: Arc::new(BlockEntityBase::new(
                &vanilla_block_entity_types::BEACON,
                level,
                pos,
                state,
            )),
            data: Arc::new(BeaconDataSlots::default()),
        }
    }

    /// Returns the values shared with the menu.
    #[must_use]
    pub fn data(&self) -> Arc<BeaconDataSlots> {
        Arc::clone(&self.data)
    }

    /// Sets the two effects, reporting whether they were allowed.
    ///
    /// Vanilla parity: the validation half of `BeaconMenu.updateEffects`,
    /// together with `BeaconBlockEntity.filterEffect`.
    #[must_use]
    pub fn set_effects(
        &self,
        primary: Option<MobEffectRef>,
        secondary: Option<MobEffectRef>,
    ) -> bool {
        let primary = filter_effect(primary);
        let secondary = filter_effect(secondary);
        if !validate_effects(primary, secondary, self.data.levels()) {
            return false;
        }
        *self.data.primary.lock() = primary;
        *self.data.secondary.lock() = secondary;
        self.set_changed();
        true
    }
}

impl BlockEntity for BeaconBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla parity: `BeaconBlockEntity.tick`, minus the beam.
    ///
    /// Vanilla walks ten blocks of the column per tick and recounts the
    /// pyramid every eighty. Steel does both on the same eighty-tick beat: the
    /// column walk only exists here to answer "is the sky clear", which nobody
    /// asks in between.
    fn tick(&self, world: &Arc<World>) {
        let game_time = u64::try_from(world.game_time()).unwrap_or(0);
        if !should_apply_this_tick(game_time) {
            return;
        }

        let pos = self.get_block_pos();
        let levels = if sky_is_clear(world, pos) {
            count_pyramid_levels(world, pos)
        } else {
            0
        };
        self.data.levels.store(levels, Ordering::Relaxed);
        if levels <= 0 {
            return;
        }

        // Vanilla drops an effect the pyramid no longer supports the next time
        // the menu validates it; doing it here means a beacon that loses a ring
        // stops handing out what it can no longer offer, rather than carrying
        // on until someone opens it.
        let primary = self.data.primary();
        let secondary = self.data.secondary();
        if !validate_effects(primary, secondary, levels) {
            return;
        }

        apply_effects(world, pos, levels, primary, secondary);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        *self.data.primary.lock() = filter_effect(effect_from_nbt(&nbt_view, "primary_effect"));
        *self.data.secondary.lock() = filter_effect(effect_from_nbt(&nbt_view, "secondary_effect"));
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        if let Some(effect) = self.data.primary() {
            nbt.insert("primary_effect", effect.key().to_string());
        }
        if let Some(effect) = self.data.secondary() {
            nbt.insert("secondary_effect", effect.key().to_string());
        }
    }
}

/// Reads one effect key out of saved beacon data.
fn effect_from_nbt(nbt: &NbtCompoundView<'_, '_>, key: &str) -> Option<MobEffectRef> {
    let name = nbt.string(key)?.to_str();
    let identifier = name.as_ref().parse::<Identifier>().ok()?;
    REGISTRY.mob_effects.by_key(&identifier)
}

/// Counts the complete rings of the pyramid under `pos`.
///
/// Vanilla parity: `BeaconBlockEntity.updateBase`. Each ring has to be square,
/// solid and made entirely of beacon base blocks; the first gap stops the
/// count, so a five-wide base with one hole in it is worth nothing above the
/// ring below it.
#[must_use]
pub fn count_pyramid_levels(world: &Arc<World>, pos: BlockPos) -> i32 {
    use crate::world::LevelReader as _;

    let mut levels = 0;
    for step in 1..=MAX_LEVELS {
        let y = pos.y() - step;
        if y < world.min_y() {
            break;
        }
        let mut complete = true;
        'ring: for x in (pos.x() - step)..=(pos.x() + step) {
            for z in (pos.z() - step)..=(pos.z() + step) {
                let state = world.get_block_state(BlockPos::new(x, y, z));
                if !REGISTRY
                    .blocks
                    .is_in_tag(state.get_block(), &BlockTag::BEACON_BASE_BLOCKS)
                {
                    complete = false;
                    break 'ring;
                }
            }
        }
        if !complete {
            break;
        }
        levels = step;
    }
    levels
}

/// Vanilla parity: the `gameTime % 80` branch of `BeaconBlockEntity.tick`.
#[must_use]
pub const fn should_apply_this_tick(game_time: u64) -> bool {
    game_time.is_multiple_of(APPLY_INTERVAL_TICKS)
}

/// How long a beacon's effect lasts, for a pyramid of `levels`.
///
/// Vanilla parity: the `(9 + levels * 2) * 20` of `applyEffects`. It is longer
/// than the four seconds between applications, so the effect never lapses
/// while a player stands in range.
#[must_use]
pub const fn effect_duration_ticks(levels: i32) -> i32 {
    (9 + levels * 2) * 20
}

/// How far a beacon's effect reaches, for a pyramid of `levels`.
///
/// Vanilla parity: the `levels * 10 + 10` of `applyEffects`.
#[must_use]
pub const fn effect_range(levels: i32) -> f64 {
    (levels * 10 + 10) as f64
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_mob_effects};

    use super::*;

    fn speed() -> MobEffectRef {
        init_vanilla_registry();
        vanilla_mob_effects::SPEED
    }

    fn regeneration() -> MobEffectRef {
        init_vanilla_registry();
        vanilla_mob_effects::REGENERATION
    }

    fn night_vision() -> MobEffectRef {
        init_vanilla_registry();
        vanilla_mob_effects::NIGHT_VISION
    }

    #[test]
    fn speed_needs_one_ring_and_regeneration_needs_four() {
        assert_eq!(required_levels_for(speed()), Some(1));
        assert_eq!(required_levels_for(regeneration()), Some(4));
    }

    /// A beacon never offers night vision, so nothing may set it.
    #[test]
    fn an_effect_no_beacon_offers_has_no_level() {
        assert_eq!(required_levels_for(night_vision()), None);
        assert!(!validate_effects(Some(night_vision()), None, MAX_LEVELS));
    }

    #[test]
    fn a_one_ring_beacon_takes_speed_but_not_regeneration() {
        assert!(validate_effects(Some(speed()), None, 1));
        assert!(!validate_effects(Some(regeneration()), None, 1));
    }

    /// Regeneration is the only effect the secondary slot holds on its own.
    #[test]
    fn a_full_beacon_takes_speed_and_regeneration() {
        assert!(validate_effects(
            Some(speed()),
            Some(regeneration()),
            MAX_LEVELS
        ));
    }

    /// The other thing the secondary slot takes is a second helping of the
    /// primary, which is what makes it stronger rather than adding to it.
    #[test]
    fn a_full_beacon_takes_the_same_effect_twice() {
        assert!(validate_effects(Some(speed()), Some(speed()), MAX_LEVELS));
    }

    /// Regeneration cannot be the primary: it sits on the fourth row, and the
    /// primary column only reaches the third.
    #[test]
    fn regeneration_cannot_be_the_primary() {
        assert!(!validate_effects(Some(regeneration()), None, MAX_LEVELS));
    }

    #[test]
    fn a_secondary_needs_the_full_pyramid() {
        assert!(!validate_effects(Some(speed()), Some(speed()), 3));
    }

    #[test]
    fn no_effect_at_all_is_valid() {
        assert!(validate_effects(None, None, 0));
    }

    /// The encoding offsets by one so that zero can mean "none"; getting that
    /// wrong would hand out whichever effect is first in the registry.
    #[test]
    fn encoding_round_trips_and_reserves_zero_for_nothing() {
        assert_eq!(encode_effect(None), 0);
        assert_eq!(decode_effect(0), None);

        let encoded = encode_effect(Some(speed()));
        assert!(encoded > 0);
        assert_eq!(
            decode_effect(encoded).map(RegistryEntry::key),
            Some(speed().key())
        );
    }

    /// The packet's id and the data slot's id differ by one. Reading one
    /// with the other's rule gives a neighboring effect rather than an error,
    /// which is exactly the kind of mistake nothing else would catch.
    #[test]
    fn the_packet_id_and_the_data_slot_id_are_one_apart() {
        let effect = speed();
        let holder_id = i32::try_from(effect.id()).expect("registry ids fit in i32");

        assert_eq!(
            effect_from_holder_id(holder_id).map(RegistryEntry::key),
            Some(effect.key())
        );
        assert_eq!(encode_effect(Some(effect)), holder_id + 1);
    }

    #[test]
    fn the_range_and_duration_grow_with_the_pyramid() {
        assert!((effect_range(1) - 20.0).abs() < f64::EPSILON);
        assert!((effect_range(4) - 50.0).abs() < f64::EPSILON);
        assert_eq!(effect_duration_ticks(1), 220);
        assert_eq!(effect_duration_ticks(4), 340);
    }

    #[test]
    fn effects_are_applied_every_four_seconds() {
        assert!(should_apply_this_tick(0));
        assert!(should_apply_this_tick(80));
        assert!(!should_apply_this_tick(79));
    }
}

/// Returns whether nothing opaque stands between the beacon and the sky.
///
/// Vanilla parity: the column walk of `BeaconBlockEntity.tick`, which clears
/// the beam the moment it meets a block that dampens light fully. Stained
/// glass and tinted glass do not, which is what lets them color the beam
/// rather than block it.
#[must_use]
pub fn sky_is_clear(world: &Arc<World>, pos: BlockPos) -> bool {
    use crate::world::LevelReader as _;

    let surface = world.world_surface_height(pos);
    let mut y = pos.y() + 1;
    while y <= surface {
        let above = BlockPos::new(pos.x(), y, pos.z());
        if world.get_block_state(above).get_light_dampening() >= FULL_LIGHT_DAMPENING {
            return false;
        }
        y += 1;
    }
    true
}

/// Hands the beacon's effects to every player in range.
///
/// Vanilla parity: `BeaconBlockEntity.applyEffects`. A full pyramid with the
/// same effect in both slots gives amplifier one instead of a second effect,
/// which is what "level II" on a beacon means.
///
/// Vanilla's reach is a box, not a sphere: a square of `range` on each side,
/// starting four blocks plus the range below the beacon and running up through
/// the whole world height. That is what the two tests below pin down.
fn apply_effects(
    world: &Arc<World>,
    pos: BlockPos,
    levels: i32,
    primary: Option<MobEffectRef>,
    secondary: Option<MobEffectRef>,
) {
    let Some(primary) = primary else {
        return;
    };

    let same_effect = secondary.is_some_and(|secondary| secondary.key() == primary.key());
    let amplifier = i32::from(levels >= MAX_LEVELS && same_effect);
    let duration = effect_duration_ticks(levels);
    let range = effect_range(levels);

    let mut in_range: Vec<Arc<Player>> = Vec::new();
    world.players.iter_players(|_, player| {
        if is_in_beacon_range(pos, range, player.position()) {
            in_range.push(player.clone());
        }
        true
    });

    for player in &in_range {
        player.add_mob_effect(
            MobEffectInstance::with_duration(primary, duration, amplifier)
                .with_ambient(true)
                .with_show_icon(true),
        );
    }

    if levels < MAX_LEVELS || same_effect {
        return;
    }
    let Some(secondary) = secondary else {
        return;
    };
    for player in &in_range {
        player.add_mob_effect(
            MobEffectInstance::with_duration(secondary, duration, 0)
                .with_ambient(true)
                .with_show_icon(true),
        );
    }
}

/// Returns whether `position` is inside a beacon's reach.
///
/// Vanilla parity: the `AABB.ofSize`-style box of `applyEffects`. Horizontally
/// it is a square, so a player standing on the diagonal is reached further out
/// than one standing on an axis.
#[must_use]
pub fn is_in_beacon_range(pos: BlockPos, range: f64, position: DVec3) -> bool {
    let dx = position.x - f64::from(pos.x());
    let dz = position.z - f64::from(pos.z());
    let dy = position.y - f64::from(pos.y());
    dx.abs() <= range + 1.0 && dz.abs() <= range + 1.0 && dy >= -(4.0 + range)
}

#[cfg(test)]
mod range_tests {
    use super::*;

    const BEACON: BlockPos = BlockPos::new(0, 100, 0);

    #[test]
    fn a_player_beside_a_one_ring_beacon_is_in_range() {
        assert!(is_in_beacon_range(
            BEACON,
            effect_range(1),
            DVec3::new(2.5, 100.0, 0.5)
        ));
    }

    #[test]
    fn a_player_past_the_range_is_out() {
        assert!(!is_in_beacon_range(
            BEACON,
            effect_range(1),
            DVec3::new(40.0, 100.0, 0.0)
        ));
    }

    /// The reach is a square, which is why the corner is further away than
    /// the side and still counts.
    #[test]
    fn the_reach_is_square_not_round() {
        let corner = DVec3::new(20.0, 100.0, 20.0);
        assert!(is_in_beacon_range(BEACON, effect_range(1), corner));
    }

    /// A beacon reaches every height above it, so a player on a tower still
    /// gets the effect.
    #[test]
    fn height_above_the_beacon_is_unbounded() {
        assert!(is_in_beacon_range(
            BEACON,
            effect_range(1),
            DVec3::new(0.5, 300.0, 0.5)
        ));
    }

    #[test]
    fn far_enough_below_is_out_of_reach() {
        assert!(!is_in_beacon_range(
            BEACON,
            effect_range(1),
            DVec3::new(0.5, 50.0, 0.5)
        ));
    }
}
