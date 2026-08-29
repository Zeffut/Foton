//! What a warden is angry about, and how angry that makes it.
//!
//! Vanilla parity: `AngerLevel` and `AngerManagement`. A warden does not have a
//! single target; it keeps a grudge against everything it has noticed, decays every
//! grudge by one a second, and acts on whichever is worst. That bookkeeping is what
//! makes the warden lose interest in a player who stands still and go for the one
//! who does not.

use std::mem;
use std::sync::Arc;

use foton_registry::sound_event::SoundEventRef;
use foton_registry::sound_events;
use foton_utils::UuidExt as _;
use rustc_hash::FxHashMap;
use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use uuid::Uuid;

use crate::entity::callback::RemovalReason;
use crate::entity::{Entity, SharedEntity};
use crate::world::World;

/// Vanilla `AngerLevel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngerLevel {
    /// Vanilla `AngerLevel.CALM`.
    Calm,
    /// Vanilla `AngerLevel.AGITATED`.
    Agitated,
    /// Vanilla `AngerLevel.ANGRY`.
    Angry,
}

impl AngerLevel {
    /// Vanilla `AngerLevel.getMinimumAnger`.
    #[must_use]
    pub const fn minimum_anger(self) -> i32 {
        match self {
            Self::Calm => 0,
            Self::Agitated => 40,
            Self::Angry => 80,
        }
    }

    /// Vanilla `AngerLevel.getAmbientSound`.
    #[must_use]
    pub const fn ambient_sound(self) -> SoundEventRef {
        match self {
            Self::Calm => &sound_events::ENTITY_WARDEN_AMBIENT,
            Self::Agitated => &sound_events::ENTITY_WARDEN_AGITATED,
            Self::Angry => &sound_events::ENTITY_WARDEN_ANGRY,
        }
    }

    /// Vanilla `AngerLevel.getListeningSound`.
    #[must_use]
    pub const fn listening_sound(self) -> SoundEventRef {
        match self {
            Self::Calm => &sound_events::ENTITY_WARDEN_LISTENING,
            Self::Agitated | Self::Angry => &sound_events::ENTITY_WARDEN_LISTENING_ANGRY,
        }
    }

    /// Vanilla `AngerLevel.byAnger`, which walks the levels from the angriest down.
    #[must_use]
    pub const fn by_anger(anger: i32) -> Self {
        if anger >= Self::Angry.minimum_anger() {
            Self::Angry
        } else if anger >= Self::Agitated.minimum_anger() {
            Self::Agitated
        } else {
            Self::Calm
        }
    }

    /// Vanilla `AngerLevel.isAngry`.
    #[must_use]
    pub const fn is_angry(self) -> bool {
        matches!(self, Self::Angry)
    }
}

/// Vanilla `AngerManagement.CONVERSION_DELAY`.
const CONVERSION_DELAY: i32 = 2;
/// Vanilla `AngerManagement.MAX_ANGER`.
const MAX_ANGER: i32 = 150;

/// Vanilla `AngerManagement`.
///
/// Deviation: vanilla hands the constructor a `Predicate<Entity>` bound to the warden
/// that owns it. Holding that predicate here would mean holding the warden, so the
/// predicate is passed in at each call instead. The stored `filter` field is the only
/// thing lost, and nothing else read it.
pub struct AngerManagement {
    conversion_delay: i32,
    highest_anger: i32,
    /// Live suspects, ordered worst first by vanilla's `Sorter`.
    suspects: Vec<SharedEntity>,
    /// Anger per live suspect, keyed by the entity id that indexes `suspects`.
    anger_by_suspect: FxHashMap<i32, i32>,
    /// Anger against entities the level has not produced yet, in insertion order.
    anger_by_uuid: Vec<(Uuid, i32)>,
}

impl Default for AngerManagement {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl AngerManagement {
    /// Vanilla `AngerManagement(Predicate, List<Pair<UUID, Integer>>)`.
    #[must_use]
    pub fn new(anger_by_uuid: Vec<(Uuid, i32)>) -> Self {
        Self {
            conversion_delay: rand::random_range(0..=CONVERSION_DELAY),
            highest_anger: 0,
            suspects: Vec::new(),
            anger_by_suspect: FxHashMap::default(),
            anger_by_uuid,
        }
    }

    /// Vanilla `AngerManagement.tick`.
    pub fn tick(&mut self, world: &Arc<World>, valid_entity: &impl Fn(&dyn Entity) -> bool) {
        self.conversion_delay -= 1;
        if self.conversion_delay <= 0 {
            self.convert_from_uuids(world);
            self.conversion_delay = CONVERSION_DELAY;
        }

        self.anger_by_uuid.retain_mut(|(_, anger)| {
            if *anger <= 1 {
                return false;
            }
            *anger -= 1;
            true
        });

        let mut expired = Vec::new();
        for suspect in &self.suspects {
            let anger = self
                .anger_by_suspect
                .get(&suspect.id())
                .copied()
                .unwrap_or(0);
            let removal_reason = suspect.removal_reason();
            if anger > 1 && valid_entity(suspect.as_ref()) && removal_reason.is_none() {
                continue;
            }
            expired.push((Arc::clone(suspect), anger, removal_reason));
        }

        for (suspect, anger, removal_reason) in expired {
            self.suspects.retain(|live| live.id() != suspect.id());
            self.anger_by_suspect.remove(&suspect.id());
            // Vanilla keeps the grudge as a UUID when the entity only left the level:
            // a player who walks out of the chunk and back is still on the warden's list.
            if anger > 1
                && matches!(
                    removal_reason,
                    Some(
                        RemovalReason::ChangedWorld
                            | RemovalReason::UnloadedToChunk
                            | RemovalReason::StoredWithPlayer
                    )
                )
            {
                self.put_uuid_anger(suspect.uuid(), anger - 1);
            }
        }

        for suspect in &self.suspects {
            if let Some(anger) = self.anger_by_suspect.get_mut(&suspect.id()) {
                *anger -= 1;
            }
        }

        self.sort_and_update_highest_anger();
    }

    /// Vanilla `AngerManagement.convertFromUuids`.
    fn convert_from_uuids(&mut self, world: &Arc<World>) {
        let mut still_missing = Vec::with_capacity(self.anger_by_uuid.len());
        for (uuid, anger) in mem::take(&mut self.anger_by_uuid) {
            match world.get_entity_by_uuid(&uuid) {
                Some(entity) => {
                    self.anger_by_suspect.insert(entity.id(), anger);
                    self.suspects.push(entity);
                }
                None => still_missing.push((uuid, anger)),
            }
        }
        self.anger_by_uuid = still_missing;
    }

    /// Vanilla `AngerManagement.sortAndUpdateHighestAnger`.
    ///
    /// Vanilla raises `highestAnger` from inside the comparator, so it ends up as the
    /// largest anger among everything the sort compared -- which for any comparison sort
    /// is every suspect -- and then special-cases the single-suspect list the comparator
    /// never runs for. Taking the maximum directly is the same answer without the
    /// side-effecting comparator.
    fn sort_and_update_highest_anger(&mut self) {
        let anger_by_suspect = &self.anger_by_suspect;
        self.highest_anger = self
            .suspects
            .iter()
            .map(|suspect| anger_by_suspect.get(&suspect.id()).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);
        self.suspects.sort_by(|left, right| {
            let left_anger = anger_by_suspect.get(&left.id()).copied().unwrap_or(0);
            let right_anger = anger_by_suspect.get(&right.id()).copied().unwrap_or(0);
            // Vanilla `AngerManagement.Sorter`: angry before agitated, players before
            // anything else at the same level, then simply angrier first.
            AngerLevel::by_anger(right_anger)
                .is_angry()
                .cmp(&AngerLevel::by_anger(left_anger).is_angry())
                .then_with(|| right.as_player().is_some().cmp(&left.as_player().is_some()))
                .then_with(|| right_anger.cmp(&left_anger))
        });
    }

    /// Vanilla `AngerManagement.increaseAnger`.
    ///
    /// Vanilla clamps to 150 before folding in whatever anger the same UUID carried from
    /// disk, so a suspect that was already hated when the chunk reloaded can exceed the
    /// cap on the tick it is recognized. That is kept.
    pub fn increase_anger(&mut self, entity: &dyn Entity, increment: i32) -> i32 {
        let is_new_suspect = !self.anger_by_suspect.contains_key(&entity.id());
        let previous = self
            .anger_by_suspect
            .get(&entity.id())
            .copied()
            .unwrap_or(0);
        let mut current_anger = MAX_ANGER.min(previous + increment);

        if is_new_suspect {
            let serialized_anger = self.take_uuid_anger(entity.uuid());
            current_anger += serialized_anger;
            if let Some(world) = entity.level()
                && let Some(shared) = world.get_entity_by_id(entity.id())
            {
                self.suspects.push(shared);
            }
        }
        self.anger_by_suspect.insert(entity.id(), current_anger);

        self.sort_and_update_highest_anger();
        current_anger
    }

    /// Vanilla `AngerManagement.clearAnger`.
    pub fn clear_anger(&mut self, entity: &dyn Entity) {
        self.anger_by_suspect.remove(&entity.id());
        self.suspects.retain(|suspect| suspect.id() != entity.id());
        self.sort_and_update_highest_anger();
    }

    /// Vanilla `AngerManagement.getActiveAnger`.
    #[must_use]
    pub fn active_anger(&self, current_target: Option<&dyn Entity>) -> i32 {
        current_target.map_or(self.highest_anger, |target| {
            self.anger_by_suspect
                .get(&target.id())
                .copied()
                .unwrap_or(0)
        })
    }

    /// Vanilla `AngerManagement.getActiveEntity`, which is `getTopSuspect` narrowed to a
    /// living entity.
    #[must_use]
    pub fn active_entity(&self, filter: &impl Fn(&dyn Entity) -> bool) -> Option<SharedEntity> {
        self.suspects
            .iter()
            .find(|suspect| filter(suspect.as_ref()))
            .filter(|suspect| suspect.as_living_entity().is_some())
            .map(Arc::clone)
    }

    /// Returns the anger recorded against one entity, live or remembered by UUID.
    #[must_use]
    pub fn anger_at(&self, entity: &dyn Entity) -> i32 {
        self.anger_by_suspect
            .get(&entity.id())
            .copied()
            .or_else(|| {
                let uuid = entity.uuid();
                self.anger_by_uuid
                    .iter()
                    .find(|(stored, _)| *stored == uuid)
                    .map(|&(_, anger)| anger)
            })
            .unwrap_or(0)
    }

    fn put_uuid_anger(&mut self, uuid: Uuid, anger: i32) {
        match self
            .anger_by_uuid
            .iter_mut()
            .find(|(stored, _)| *stored == uuid)
        {
            Some(entry) => entry.1 = anger,
            None => self.anger_by_uuid.push((uuid, anger)),
        }
    }

    fn take_uuid_anger(&mut self, uuid: Uuid) -> i32 {
        let Some(index) = self
            .anger_by_uuid
            .iter()
            .position(|(stored, _)| *stored == uuid)
        else {
            return 0;
        };
        self.anger_by_uuid.remove(index).1
    }

    /// Writes vanilla's `AngerManagement.codec` shape.
    ///
    /// Vanilla concatenates the live suspects with the ones still known only by UUID, so
    /// a warden saved mid-grudge keeps both halves.
    pub fn save(&self, nbt: &mut NbtCompound) {
        let mut suspects = Vec::with_capacity(self.suspects.len() + self.anger_by_uuid.len());
        for suspect in &self.suspects {
            let anger = self
                .anger_by_suspect
                .get(&suspect.id())
                .copied()
                .unwrap_or(0);
            suspects.push(suspect_entry(suspect.uuid(), anger));
        }
        for &(uuid, anger) in &self.anger_by_uuid {
            suspects.push(suspect_entry(uuid, anger));
        }
        let mut anger = NbtCompound::new();
        anger.insert("suspects", NbtTag::List(NbtList::Compound(suspects)));
        nbt.insert("anger", anger);
    }

    /// Reads vanilla's `AngerManagement.codec` shape.
    #[must_use]
    pub fn load(nbt: &NbtCompoundView<'_, '_>) -> Self {
        let Some(anger) = nbt.compound("anger") else {
            return Self::default();
        };
        let Some(suspects) = anger.list("suspects").and_then(|list| list.compounds()) else {
            return Self::default();
        };

        Self::new(
            suspects
                .into_iter()
                .filter_map(|suspect| {
                    let uuid = Uuid::from_int_array(&suspect.int_array("uuid")?)?;
                    let anger = suspect.int("anger")?;
                    (anger >= 0).then_some((uuid, anger))
                })
                .collect(),
        )
    }
}

fn suspect_entry(uuid: Uuid, anger: i32) -> NbtCompound {
    let mut entry = NbtCompound::new();
    entry.insert("uuid", NbtTag::IntArray(uuid.to_int_array().to_vec()));
    entry.insert("anger", anger);
    entry
}
