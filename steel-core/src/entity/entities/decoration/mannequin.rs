//! The mannequin.
//!
//! Vanilla parity: `Mannequin`. A player-shaped statue new in 26.2: it wears a
//! profile's skin, holds a pose, can be nailed in place, and carries a label
//! above its head. It has no AI at all -- like the armor stand it is a
//! `LivingEntity` that is not a mob -- and its whole behaviour is the state it
//! remembers.
//!
//! Vanilla splits it from the player through a shared `Avatar` superclass.
//! Steel's `Player` predates that split, so the two are separate here; the
//! `AvatarEntityData` layer both share is generated, and this entity is the
//! only thing on it so far.
//!
//! Not implemented: `applyImplicitComponents` for `minecraft:profile`, which is
//! how a placed mannequin item carries a skin onto the entity. Steel's spawn
//! path has no item component to read, the same gap the command block's
//! `BLOCK_ENTITY_DATA` has -- a summoned mannequin still loads and saves its
//! profile through NBT.

use std::sync::Weak;
use std::sync::atomic::{AtomicBool, Ordering};

use glam::DVec3;
use simdnbt::FromNbtTag as _;
use simdnbt::ToNbtTag as _;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_macros::entity_behavior;
use steel_registry::entity_data::{EntityPose, HumanoidArm};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::resolvable_profile::ResolvableProfile;
use steel_registry::vanilla_entity_data::MannequinEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey, translations};
use text_components::TextComponent;

use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase,
};
use crate::world::World;

/// The seven parts of a player skin that can be hidden.
///
/// Vanilla parity: `PlayerModelPart`, whose mask is `1 << bit` and whose
/// serialized name is what the `hidden_layers` list stores.
const MODEL_PARTS: [(&str, i8); 7] = [
    ("cape", 1 << 0),
    ("jacket", 1 << 1),
    ("left_sleeve", 1 << 2),
    ("right_sleeve", 1 << 3),
    ("left_pants_leg", 1 << 4),
    ("right_pants_leg", 1 << 5),
    ("hat", 1 << 6),
];

/// Every model part shown.
///
/// Vanilla parity: `Mannequin.ALL_LAYERS`. The synced byte lists the parts that
/// are *shown*, so "all layers" is every bit set and the saved `hidden_layers`
/// list is its complement.
const ALL_LAYERS: i8 = 0b0111_1111;

/// The poses a mannequin may be put into.
///
/// Vanilla parity: `Mannequin.VALID_POSES`. Anything else in the `pose` tag is
/// rejected by the codec and falls back to standing.
const VALID_POSES: [EntityPose; 5] = [
    EntityPose::Standing,
    EntityPose::Sneaking,
    EntityPose::Swimming,
    EntityPose::FallFlying,
    EntityPose::Sleeping,
];

/// State a mannequin keeps that is not mirrored to clients.
///
/// The synced description is the *effective* one -- absent when hidden -- so
/// the description itself has to be kept separately or hiding it would lose it.
struct MannequinState {
    description: TextComponent,
    hide_description: bool,
}

/// A mannequin.
#[entity_behavior(class = "Mannequin")]
pub struct MannequinEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    entity_data: SyncMutex<MannequinEntityData>,
    state: SyncMutex<MannequinState>,
    /// Whether the mannequin refuses to be moved at all.
    immovable: AtomicBool,
}

// SAFETY: This key is owned by Steel and uniquely identifies `MannequinEntity`.
unsafe impl DowncastType for MannequinEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/mannequin");
}

impl MannequinEntity {
    /// Creates a mannequin at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a mannequin from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mut entity_data = MannequinEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        // Vanilla parity: the constructor sets every model layer visible.
        entity_data
            .avatar_mut()
            .player_mode_customisation
            .set(ALL_LAYERS);

        Self {
            base,
            entity_type,
            living_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(MannequinState {
                description: default_description(),
                hide_description: false,
            }),
            immovable: AtomicBool::new(false),
        }
    }

    /// Returns the profile whose skin this mannequin wears.
    #[must_use]
    pub fn profile(&self) -> ResolvableProfile {
        self.entity_data.lock().profile.get().clone()
    }

    /// Sets the profile whose skin this mannequin wears.
    pub fn set_profile(&self, profile: ResolvableProfile) {
        self.entity_data.lock().profile.set(profile);
    }

    /// Returns whether this mannequin refuses to be moved.
    ///
    /// Vanilla parity: `Mannequin.getImmovable`.
    #[must_use]
    pub fn is_immovable(&self) -> bool {
        self.immovable.load(Ordering::Relaxed)
    }

    /// Sets whether this mannequin refuses to be moved.
    pub fn set_immovable(&self, immovable: bool) {
        self.immovable.store(immovable, Ordering::Relaxed);
        self.entity_data.lock().immovable.set(immovable);
    }

    /// Returns the bit set of model parts this mannequin shows.
    #[must_use]
    pub fn shown_model_parts(&self) -> i8 {
        *self
            .entity_data
            .lock()
            .avatar()
            .player_mode_customisation
            .get()
    }

    /// Sets the bit set of model parts this mannequin shows.
    pub fn set_shown_model_parts(&self, parts: i8) {
        self.entity_data
            .lock()
            .avatar_mut()
            .player_mode_customisation
            .set(parts);
    }

    /// Returns which hand this mannequin holds items in.
    #[must_use]
    pub fn main_arm(&self) -> HumanoidArm {
        *self.entity_data.lock().avatar().player_main_hand.get()
    }

    /// Sets which hand this mannequin holds items in.
    pub fn set_main_arm(&self, arm: HumanoidArm) {
        self.entity_data
            .lock()
            .avatar_mut()
            .player_main_hand
            .set(arm);
    }

    /// Returns the label shown above this mannequin, if it is shown at all.
    ///
    /// Vanilla parity: `Mannequin.getDescription`, which reads the synced
    /// optional rather than the stored one -- a hidden description reads as
    /// nothing even though the text is still remembered.
    #[must_use]
    pub fn description(&self) -> Option<TextComponent> {
        self.entity_data
            .lock()
            .description
            .get()
            .as_ref()
            .map(|description| (**description).clone())
    }

    /// Sets the label text, without changing whether it is shown.
    pub fn set_description(&self, description: TextComponent) {
        self.state.lock().description = description;
        self.update_description();
    }

    /// Sets whether the label is shown, without forgetting its text.
    pub fn set_hide_description(&self, hide: bool) {
        self.state.lock().hide_description = hide;
        self.update_description();
    }

    /// Vanilla parity: `Mannequin.updateDescription`.
    fn update_description(&self) {
        let state = self.state.lock();
        let effective = if state.hide_description {
            None
        } else {
            Some(Box::new(state.description.clone()))
        };
        drop(state);
        self.entity_data.lock().description.set(effective);
    }

    /// Returns the pose this mannequin is holding.
    #[must_use]
    pub fn pose(&self) -> EntityPose {
        *self.entity_data.lock().base().pose.get()
    }

    /// Puts this mannequin into `pose`, if it is one a mannequin may hold.
    ///
    /// Vanilla parity: `Mannequin.POSE_CODEC`, which rejects anything outside
    /// [`VALID_POSES`] rather than letting a map put a mannequin into, say, a
    /// warden's digging pose.
    pub fn set_pose_checked(&self, pose: EntityPose) {
        let pose = if VALID_POSES.contains(&pose) {
            pose
        } else {
            EntityPose::Standing
        };
        self.entity_data.lock().base_mut().pose.set(pose);
    }
}

/// Returns the label a mannequin shows unless one is set.
///
/// Vanilla parity: `Mannequin.DEFAULT_DESCRIPTION`.
fn default_description() -> TextComponent {
    TextComponent::translated(translations::ENTITY_MINECRAFT_MANNEQUIN_LABEL.msg())
}

/// Returns the model-part mask the `hidden_layers` list describes.
///
/// Vanilla parity: `Mannequin.LAYERS_CODEC`, which stores the *hidden* parts
/// and reconstructs the shown mask by clearing each one from `ALL_LAYERS`.
fn shown_parts_from_hidden(names: &[String]) -> i8 {
    let mut shown = ALL_LAYERS;
    for name in names {
        if let Some((_, mask)) = MODEL_PARTS.iter().find(|(part, _)| part == name) {
            shown &= !mask;
        }
    }
    shown
}

/// Returns the `hidden_layers` list for a shown-part mask.
fn hidden_parts_from_shown(shown: i8) -> Vec<String> {
    MODEL_PARTS
        .iter()
        .filter(|(_, mask)| shown & mask == 0)
        .map(|(part, _)| (*part).to_owned())
        .collect()
}

/// Returns the serialized name of a pose.
const fn pose_name(pose: EntityPose) -> &'static str {
    match pose {
        EntityPose::Sneaking => "crouching",
        EntityPose::Swimming => "swimming",
        EntityPose::FallFlying => "fall_flying",
        EntityPose::Sleeping => "sleeping",
        _ => "standing",
    }
}

/// Returns the pose a serialized name describes, if a mannequin may hold it.
fn pose_from_name(name: &str) -> EntityPose {
    match name {
        "crouching" => EntityPose::Sneaking,
        "swimming" => EntityPose::Swimming,
        "fall_flying" => EntityPose::FallFlying,
        "sleeping" => EntityPose::Sleeping,
        _ => EntityPose::Standing,
    }
}

impl Entity for MannequinEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Mannequin.isEffectiveAi`, which an immovable mannequin
    /// refuses -- that is what stops knockback and pushing moving it.
    fn is_effective_ai(&self) -> bool {
        !self.is_immovable()
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert("profile", self.profile().to_nbt_tag());

        let hidden = hidden_parts_from_shown(self.shown_model_parts());
        nbt.insert(
            "hidden_layers",
            NbtTag::List(NbtList::String(
                hidden.into_iter().map(Into::into).collect(),
            )),
        );

        nbt.insert(
            "main_hand",
            match self.main_arm() {
                HumanoidArm::Left => "left",
                HumanoidArm::Right => "right",
            },
        );
        nbt.insert("pose", pose_name(self.pose()));
        nbt.insert("immovable", self.is_immovable());

        // Vanilla parity: the default label is not written, and a hidden one is
        // recorded as a flag rather than as an absent description -- the two
        // cases have to stay distinguishable across a save.
        let state = self.state.lock();
        if state.hide_description {
            nbt.insert("hide_description", true);
        } else if state.description != default_description() {
            nbt.insert("description", state.description.to_codec_nbt());
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        if let Some(profile) = nbt.get("profile").and_then(ResolvableProfile::from_nbt_tag) {
            self.set_profile(profile);
        }

        let hidden: Vec<String> = nbt.list("hidden_layers").map_or_else(Vec::new, |list| {
            list.strings()
                .map(|values| values.iter().map(ToString::to_string).collect())
                .unwrap_or_default()
        });
        self.set_shown_model_parts(shown_parts_from_hidden(&hidden));

        let arm = match nbt.string("main_hand").map(ToString::to_string).as_deref() {
            Some("left") => HumanoidArm::Left,
            _ => HumanoidArm::Right,
        };
        self.set_main_arm(arm);

        let pose = nbt.string("pose").map_or(EntityPose::Standing, |value| {
            pose_from_name(&value.to_str())
        });
        self.set_pose_checked(pose);

        self.set_immovable(nbt.byte("immovable").is_some_and(|value| value != 0));
        self.set_hide_description(nbt.byte("hide_description").is_some_and(|value| value != 0));
        let description = nbt
            .get("description")
            .map(|tag| tag.to_owned())
            .as_ref()
            .and_then(TextComponent::from_nbt)
            .unwrap_or_else(default_description);
        self.set_description(description);
    }
}

impl LivingEntity for MannequinEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let clamped = health.clamp(0.0, self.get_max_health());
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    /// Vanilla parity: `Mannequin.isImmobile`, which is what nails an immovable
    /// mannequin down.
    fn is_immobile(&self) -> bool {
        self.is_immovable() || self.get_health() <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    use std::string::ToString;

    use super::*;
    use crate::entity::next_entity_id;

    fn mannequin() -> MannequinEntity {
        init_vanilla_registry();
        MannequinEntity::new(
            &vanilla_entities::MANNEQUIN,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        )
    }

    fn reload(nbt: &NbtCompound) -> MannequinEntity {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let entity = mannequin();
        entity.load_additional((&borrowed).into());
        entity
    }

    /// The saved list holds the *hidden* layers while the synced byte holds the
    /// shown ones, so the two are complements. Getting that backwards would
    /// make every mannequin load inside-out.
    #[test]
    fn hidden_layers_are_the_complement_of_the_shown_mask() {
        assert_eq!(shown_parts_from_hidden(&[]), ALL_LAYERS);
        assert!(hidden_parts_from_shown(ALL_LAYERS).is_empty());

        let hat_hidden = shown_parts_from_hidden(&["hat".to_owned()]);
        assert_eq!(hat_hidden, ALL_LAYERS & !(1 << 6));
        assert_eq!(hidden_parts_from_shown(hat_hidden), vec!["hat".to_owned()]);
    }

    /// A mannequin may only hold five of the eighteen poses; anything else
    /// falls back to standing rather than putting it into a warden's crouch.
    #[test]
    fn an_invalid_pose_falls_back_to_standing() {
        let entity = mannequin();
        entity.set_pose_checked(EntityPose::Swimming);
        assert_eq!(entity.pose(), EntityPose::Swimming);

        entity.set_pose_checked(EntityPose::Digging);
        assert_eq!(entity.pose(), EntityPose::Standing);
    }

    /// Hiding the label must not lose it: vanilla keeps the text and syncs an
    /// absent description, so unhiding brings the same text back.
    #[test]
    fn hiding_the_label_keeps_its_text() {
        let entity = mannequin();
        entity.set_description(TextComponent::plain("Shopkeeper"));
        assert_eq!(
            entity.description(),
            Some(TextComponent::plain("Shopkeeper"))
        );

        entity.set_hide_description(true);
        assert_eq!(entity.description(), None);

        entity.set_hide_description(false);
        assert_eq!(
            entity.description(),
            Some(TextComponent::plain("Shopkeeper"))
        );
    }

    /// Everything a map-maker sets has to come back after a chunk reload.
    #[test]
    fn a_posed_immovable_mannequin_round_trips() {
        let entity = mannequin();
        entity.set_pose_checked(EntityPose::Sleeping);
        entity.set_immovable(true);
        entity.set_main_arm(HumanoidArm::Left);
        entity.set_shown_model_parts(shown_parts_from_hidden(&["cape".to_owned()]));
        entity.set_description(TextComponent::plain("Guard"));

        let mut nbt = NbtCompound::new();
        entity.save_additional(&mut nbt);
        assert_eq!(nbt.byte("immovable"), Some(1));
        assert_eq!(
            nbt.string("pose").map(ToString::to_string),
            Some("sleeping".to_owned())
        );
        assert_eq!(
            nbt.string("main_hand").map(ToString::to_string),
            Some("left".to_owned())
        );

        let reloaded = reload(&nbt);
        assert_eq!(reloaded.pose(), EntityPose::Sleeping);
        assert!(reloaded.is_immovable());
        assert_eq!(reloaded.main_arm(), HumanoidArm::Left);
        assert_eq!(
            reloaded.shown_model_parts(),
            shown_parts_from_hidden(&["cape".to_owned()])
        );
        assert_eq!(reloaded.description(), Some(TextComponent::plain("Guard")));
    }

    /// A hidden label survives as a flag, not as a missing description -- the
    /// two have to stay distinguishable or unhiding would show the default.
    #[test]
    fn a_hidden_label_stays_hidden_across_a_save() {
        let entity = mannequin();
        entity.set_description(TextComponent::plain("Guard"));
        entity.set_hide_description(true);

        let mut nbt = NbtCompound::new();
        entity.save_additional(&mut nbt);
        assert_eq!(nbt.byte("hide_description"), Some(1));
        assert!(nbt.get("description").is_none());

        assert_eq!(reload(&nbt).description(), None);
    }

    /// An immovable mannequin is nailed down: vanilla expresses that as
    /// `isImmobile` plus `isEffectiveAi`, and both have to move together or a
    /// shove would still slide it.
    #[test]
    fn an_immovable_mannequin_is_immobile_and_runs_no_ai() {
        let entity = mannequin();
        assert!(!entity.is_immobile());
        assert!(entity.is_effective_ai());

        entity.set_immovable(true);
        assert!(entity.is_immobile());
        assert!(!entity.is_effective_ai());
    }
}
