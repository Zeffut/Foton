//! Mobs that fit in a bucket.
//!
//! Vanilla parity: `net.minecraft.world.entity.Bucketable`. The interface is
//! two halves that have to agree: the mob writes itself into a bucket when a
//! player scoops it up, and `MobBucketItem` reads it back out when the bucket
//! is emptied. Steel had only the second half, and it spawned a fresh mob
//! rather than the one that went in; this is the first half plus the state the
//! second half was missing.

use std::io::Cursor;

use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::NbtCompound;
use steel_registry::data_components::CustomData;
use steel_registry::data_components::vanilla_components;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_items;
use steel_utils::types::InteractionHand;

use crate::advancement::triggers;
use crate::behavior::InteractionResult;
use crate::entity::{Entity, LivingEntity, Mob, RemovalReason};
use crate::player::Player;

/// A mob a water bucket can pick up.
///
/// Vanilla parity: the `Bucketable` interface. Vanilla's two `saveDefaultData`
/// / `loadDefaultData` statics are [`save_default_data_to_bucket_tag`] and
/// [`load_default_data_from_bucket_tag`] here, because Rust has no static
/// interface methods.
pub trait Bucketable: LivingEntity {
    /// Vanilla parity: `Bucketable.fromBucket`.
    #[expect(
        clippy::wrong_self_convention,
        reason = "this is vanilla's `fromBucket()` getter, not a conversion"
    )]
    fn from_bucket(&self) -> bool;

    /// Vanilla parity: `Bucketable.setFromBucket`.
    fn set_from_bucket(&self, from_bucket: bool);

    /// Vanilla parity: `Bucketable.saveToBucketTag`.
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack);

    /// Vanilla parity: `Bucketable.loadFromBucketTag`.
    fn load_from_bucket_tag(&self, tag: BorrowedNbtCompoundView<'_, '_>);

    /// Vanilla parity: `Bucketable.getBucketItemStack`.
    fn bucket_item_stack(&self) -> ItemStack;

    /// Vanilla parity: `Bucketable.getPickupSound`.
    fn pickup_sound(&self) -> SoundEventRef;

    /// Vanilla parity: `Bucketable.canBePickedUpWithBucket`.
    fn can_be_picked_up_with_bucket(&self, item_stack: &ItemStack) -> bool {
        item_stack.is(&vanilla_items::WATER_BUCKET)
    }
}

/// Writes the flags every bucketed mob carries into the bucket.
///
/// Vanilla parity: `Bucketable.saveDefaultDataToBucketTag`. Each flag is only
/// written when it is set, so an ordinary mob leaves an all-but-empty tag
/// behind -- which is why the bucket of a plain fish stacks with another.
pub fn save_default_data_to_bucket_tag(mob: &dyn Mob, bucket: &mut ItemStack) {
    if let Some(custom_name) = mob.custom_name() {
        bucket.set(vanilla_components::CUSTOM_NAME, custom_name);
    }

    let mut tag = NbtCompound::new();
    if mob.is_no_ai() {
        tag.insert("NoAI", true);
    }
    if mob.is_silent() {
        tag.insert("Silent", true);
    }
    if mob.is_no_gravity() {
        tag.insert("NoGravity", true);
    }
    if mob.has_glowing_tag() {
        tag.insert("Glowing", true);
    }
    if mob.is_invulnerable() {
        tag.insert("Invulnerable", true);
    }
    if mob.is_persistence_required() {
        tag.insert("PersistenceRequired", true);
    }
    tag.insert("Health", mob.get_health());

    set_bucket_entity_data(bucket, tag);
}

/// Reads those flags back out of the bucket.
///
/// Vanilla parity: `Bucketable.loadDefaultDataFromBucketTag`. Vanilla's
/// `ifPresent` shape matters: a key that is absent leaves the mob's own default
/// alone rather than clearing it.
pub fn load_default_data_from_bucket_tag(mob: &dyn Mob, tag: BorrowedNbtCompoundView<'_, '_>) {
    if let Some(no_ai) = tag.byte("NoAI") {
        mob.set_no_ai(no_ai != 0);
    }
    if let Some(silent) = tag.byte("Silent") {
        mob.set_silent(silent != 0);
    }
    if let Some(no_gravity) = tag.byte("NoGravity") {
        mob.set_no_gravity(no_gravity != 0);
    }
    if let Some(glowing) = tag.byte("Glowing") {
        mob.set_glowing_tag(glowing != 0);
    }
    if let Some(invulnerable) = tag.byte("Invulnerable") {
        mob.set_invulnerable(invulnerable != 0);
    }
    if tag
        .byte("PersistenceRequired")
        .is_some_and(|flag| flag != 0)
    {
        mob.set_persistence_required();
    }
    if let Some(health) = tag.float("Health") {
        mob.set_health(health);
    }
}

/// Merges `tag` into a bucket's `minecraft:bucket_entity_data`.
///
/// Vanilla parity: `CustomData.update(DataComponents.BUCKET_ENTITY_DATA, ...)`,
/// which reads the component, hands the tag to the mutator and writes it back.
pub fn set_bucket_entity_data(bucket: &mut ItemStack, tag: NbtCompound) {
    let Some(custom_data) = CustomData::try_from_compound(tag) else {
        return;
    };
    bucket.set(vanilla_components::BUCKET_ENTITY_DATA, custom_data);
}

/// Hands a bucket's `minecraft:bucket_entity_data` to `read` as a borrowed view.
///
/// Vanilla parity: the
/// `itemStack.getOrDefault(BUCKET_ENTITY_DATA, CustomData.EMPTY).copyTag()` of
/// `MobBucketItem.spawn`. The component stores owned NBT and every loader in
/// Steel reads the borrowed shape, so the tag makes one round trip through
/// bytes here rather than in each caller.
pub fn read_bucket_entity_data(bucket: &ItemStack, read: impl FnOnce(BorrowedNbtCompoundView)) {
    let Some(saved) = bucket.get(vanilla_components::BUCKET_ENTITY_DATA) else {
        return;
    };
    let mut bytes = Vec::new();
    saved.copy_tag().write(&mut bytes);
    let Ok(borrowed) = read_borrowed_compound(&mut Cursor::new(bytes.as_slice())) else {
        return;
    };
    read(BorrowedNbtCompoundView::from(&borrowed));
}

/// Picks a mob up with the water bucket in `hand`.
///
/// Vanilla parity: `Bucketable.bucketMobPickup`. Returns `None` -- vanilla's
/// `Optional.empty()` -- when the held item is not a bucket this mob answers
/// to, so the caller falls through to its own interaction.
#[must_use]
pub fn bucket_mob_pickup<T: Bucketable + Mob + ?Sized>(
    player: &Player,
    hand: InteractionHand,
    pickup_entity: &T,
) -> Option<InteractionResult> {
    let held = {
        let inventory = player.inventory.lock();
        let held = inventory.get_item_in_hand(hand);
        held.copy_with_count(held.count())
    };
    if !pickup_entity.can_be_picked_up_with_bucket(&held) || !Entity::is_alive(pickup_entity) {
        return None;
    }

    pickup_entity.play_sound(pickup_entity.pickup_sound(), 1.0, 1.0);
    let mut bucket = pickup_entity.bucket_item_stack();
    pickup_entity.save_to_bucket_tag(&mut bucket);

    // Vanilla parity: `ItemUtils.createFilledResult(itemStack, player, bucket,
    // false)` -- the `false` is `limitCreativeStackSize`, so a creative player
    // still gets the filled bucket rather than nothing.
    // The filled bucket is moved into the inventory below, and the trigger
    // wants the same stack vanilla hands it.
    let filled = bucket.clone();
    let overflow = {
        let mut inventory = player.inventory.lock();
        inventory.apply_filled_result(hand, bucket, player.has_infinite_materials(), false)
    };
    if !overflow.is_empty() {
        let _ = player.drop_item(overflow, false, false);
    }

    triggers::item::filled_bucket(player, &filled);
    pickup_entity.drop_leash();
    pickup_entity.set_removed(RemovalReason::Discarded);
    Some(InteractionResult::Success)
}
