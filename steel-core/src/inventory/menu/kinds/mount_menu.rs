//! The mount inventory screen.
//!
//! Vanilla parity: `AbstractMountInventoryMenu` and the two subclasses that
//! only differ in their slot sprites and in which mount they cast to --
//! `HorseInventoryMenu` and `NautilusInventoryMenu`. Steel builds one menu for
//! both and takes the cast as a parameter.
//!
//! Slot layout:
//! - Slot 0: the saddle, an equipment slot of the mount itself
//! - Slot 1: the body armor, likewise
//! - Slots 2..2+`columns`*3: the cargo grid, empty for a mount with no chest
//! - the player's 36 slots after that
//!
//! Two things make this screen unlike every other menu. It is opened by
//! [`CMountScreenOpen`] rather than by the open-screen packet, because the
//! client builds the menu from the entity it already tracks; and its first two
//! slots are not storage of their own but the mount's worn equipment, which is
//! why they sit on [`LivingEntityBase::equipment_slot_container`].

use std::sync::Arc;

use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::{Entity, SharedEntity, WeakEntity};
use crate::inventory::prelude::*;
use crate::inventory::slots::{ArmorSlot, SlotStorage};
use crate::player::player_inventory::PlayerInventory;

/// Reads the inventory a mount owns right now.
///
/// Vanilla parity: the `hasInventoryChanged` override each mount menu carries.
/// It is nothing but a cast to the concrete mount, so Steel passes the cast in
/// rather than splitting the menu in two for it. A mount that grew or lost its
/// chest has replaced its inventory, and the screen has to close.
pub type MountInventoryProbe = fn(&dyn Entity) -> Option<ContainerId>;

/// Reads the current inventory of a horse-shaped mount.
///
/// Vanilla parity: `HorseInventoryMenu.hasInventoryChanged`.
#[must_use]
pub fn horse_inventory(entity: &dyn Entity) -> Option<ContainerId> {
    let horse = entity.as_abstract_horse()?;
    Some(ContainerId::from_arc(
        &horse.abstract_horse_base().inventory(),
    ))
}

/// Reads the current inventory of a nautilus.
///
/// Vanilla parity: `NautilusInventoryMenu.hasInventoryChanged`.
#[must_use]
pub fn nautilus_inventory(entity: &dyn Entity) -> Option<ContainerId> {
    let nautilus = entity.as_abstract_nautilus()?;
    Some(ContainerId::from_arc(
        &nautilus.abstract_nautilus_base().inventory(),
    ))
}

/// Opens `mount`'s own inventory screen for `player`.
///
/// Vanilla parity: `ServerPlayer.openHorseInventory` and
/// `ServerPlayer.openNautilusInventory`, which are `openMenu` with the mount
/// screen packet in place of the open-screen one. The tame and rider gates
/// belong to `openCustomInventoryScreen` and have already run by here.
pub fn open_mount_screen(
    mount: &SharedEntity,
    inventory: Shared<SimpleContainer>,
    inventory_columns: usize,
    current_inventory: MountInventoryProbe,
    player: &Player,
) {
    let Some(living) = mount.as_living_entity() else {
        return;
    };
    let (equipment, saddle_index) = living
        .living_base()
        .equipment_slot_container(EquipmentSlot::Saddle);
    let body_index = living
        .living_base()
        .equipment_slot_container(EquipmentSlot::Body)
        .1;
    let entity_id = Entity::id(mount.as_ref());
    let mount = Arc::clone(mount);

    // The mount screen carries no title: the client names it after the entity.
    player.open_menu("", move |context| {
        mount_menu(MountMenuParts {
            player_inventory: context.player.inventory.clone(),
            container_id: context.container_id,
            mount,
            entity_id,
            equipment: ContainerRef::from(equipment),
            saddle_index,
            body_index,
            inventory,
            inventory_columns,
            current_inventory,
        })
    });
}

/// Everything [`mount_menu`] needs, resolved before the menu is built.
struct MountMenuParts {
    player_inventory: Shared<PlayerInventory>,
    container_id: u8,
    mount: SharedEntity,
    entity_id: i32,
    /// The mount's own equipment, which backs the saddle and armor slots.
    equipment: ContainerRef,
    saddle_index: usize,
    body_index: usize,
    inventory: Shared<SimpleContainer>,
    inventory_columns: usize,
    current_inventory: MountInventoryProbe,
}

/// Builds the mount screen.
fn mount_menu(parts: MountMenuParts) -> Menu {
    let MountMenuParts {
        player_inventory,
        container_id,
        mount,
        entity_id,
        equipment,
        saddle_index,
        body_index,
        inventory,
        inventory_columns,
        current_inventory,
    } = parts;

    let weak_mount = Arc::downgrade(&mount);
    let inventory = ContainerRef::from(inventory);
    let mut builder = MenuBuilder::new(None, container_id);
    builder.mount_screen(inventory_columns, entity_id);

    let saddle = builder.section_at(
        &equipment,
        [saddle_index],
        equipment_slots(&weak_mount, EquipmentSlot::Saddle),
    );
    let body = builder.section_at(
        &equipment,
        [body_index],
        equipment_slots(&weak_mount, EquipmentSlot::Body),
    );
    let cargo = builder.section_all(&inventory);
    let player = builder.player_inventory(&player_inventory);

    builder.build(MountKind {
        mount: weak_mount,
        inventory,
        current_inventory,
        saddle,
        body,
        cargo,
        main: player.main(),
        hotbar: player.hotbar(),
        player: player.all(),
    })
}

/// A section kind that builds [`MountEquipmentSlot`]s for `slot`.
fn equipment_slots(mount: &WeakEntity, slot: EquipmentSlot) -> SectionKind {
    let mount = mount.clone();
    SectionKind::custom(move |container, index| {
        Box::new(MountEquipmentSlot {
            base: ArmorSlot::new(container.clone(), index, slot),
            mount: mount.clone(),
        })
    })
}

/// The saddle or body-armor slot of a mount screen.
///
/// Vanilla parity: the `ArmorSlot` that `HorseInventoryMenu` builds over
/// `Mob.createEquipmentSlotContainer`. It differs from the player's own
/// [`ArmorSlot`] by having an owner: what may be placed depends on the mount,
/// and equipping something has to reach it.
///
/// Vanilla also switches these two slots off through `Slot.isActive` -- an
/// unsaddleable mount shows the saddle slot greyed out. That is a client-side
/// decision: nothing on the server reads `isActive`, and the client rebuilds
/// this menu itself, so Steel has nothing to send for it.
struct MountEquipmentSlot {
    base: ArmorSlot,
    mount: WeakEntity,
}

// SAFETY: This Steel-owned key uniquely identifies `MountEquipmentSlot`.
unsafe impl DowncastType for MountEquipmentSlot {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:slot/mount_equipment");
}

impl Slot for MountEquipmentSlot {
    fn storage(&self) -> &SlotStorage {
        self.base.storage()
    }

    fn get_item<'a>(&self, guard: &'a ContainerLockGuard) -> &'a ItemStack {
        self.base.get_item(guard)
    }

    fn get_item_mut<'a>(&self, guard: &'a mut ContainerLockGuard) -> &'a mut ItemStack {
        self.base.get_item_mut(guard)
    }

    fn set_item(&self, guard: &mut ContainerLockGuard, stack: ItemStack) {
        self.base.set_item(guard, stack);
    }

    /// Vanilla parity: `ArmorSlot.setByPlayer` followed by the `setTheItem` of
    /// `Mob.createEquipmentSlotContainer`, which is what the write reaches
    /// through.
    ///
    /// The mount's own locks are taken with the containers released: the item
    /// being written lives in the mount's equipment, so playing the equip
    /// sound under the menu's lock would take that same mutex twice.
    fn set_by_player(
        &self,
        guard: &mut ContainerLockGuard,
        stack: ItemStack,
        previous: &ItemStack,
    ) {
        let slot = self.base.equipment_slot();
        if let Some(mount) = self.mount.upgrade() {
            let equipped = stack.clone();
            guard.run_unlocked(|| {
                if let Some(living) = mount.as_living_entity() {
                    living.on_equip_item(slot, previous, &equipped);
                }
                if !equipped.is_empty()
                    && let Some(mob) = mount.as_mob()
                {
                    mob.set_guaranteed_drop(slot);
                    mob.set_persistence_required();
                }
            });
        }
        self.base.set_item(guard, stack);
    }

    /// Vanilla parity: `ArmorSlot.mayPlace`, whose `isEquippableInSlot` is the
    /// mount's -- horse armor does not fit a llama and llama armor does not fit
    /// a horse.
    fn may_place(&self, stack: &ItemStack) -> bool {
        let Some(mount) = self.mount.upgrade() else {
            return false;
        };
        let Some(living) = mount.as_living_entity() else {
            return false;
        };
        living.is_equippable_in_slot(stack, self.base.equipment_slot())
    }

    fn may_pickup(&self, guard: &ContainerLockGuard, player: &Player) -> bool {
        self.base.may_pickup(guard, player)
    }

    fn get_max_stack_size(&self, guard: &ContainerLockGuard) -> i32 {
        self.base.get_max_stack_size(guard)
    }

    fn set_changed(&self, guard: &mut ContainerLockGuard) {
        self.base.set_changed(guard);
    }

    fn get_container_slot(&self) -> usize {
        self.base.get_container_slot()
    }
}

/// Per-menu mount state: who the screen belongs to and which inventory it was
/// built over.
pub struct MountKind {
    mount: WeakEntity,
    /// The mount inventory this menu was built over.
    inventory: ContainerRef,
    current_inventory: MountInventoryProbe,
    saddle: Section,
    body: Section,
    cargo: Section,
    main: Section,
    hotbar: Section,
    player: Section,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl DowncastType for MountKind {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:menu/mount");
}

impl MenuKind for MountKind {
    /// Vanilla parity: `AbstractMountInventoryMenu.stillValid`.
    ///
    /// The inventory check is the interesting one: strapping a chest onto a
    /// donkey replaces its inventory with a wider one, and the screen showing
    /// the old one has to close rather than keep writing into it.
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        let Some(mount) = self.mount.upgrade() else {
            return false;
        };
        (self.current_inventory)(mount.as_ref()) == Some(self.inventory.container_id())
            && self.inventory.still_valid(player)
            && Entity::is_alive(mount.as_ref())
            && player.is_within_entity_interaction_range(mount.bounding_box(), 4.0)
    }

    /// Vanilla parity: `AbstractMountInventoryMenu.quickMoveStack`.
    ///
    /// The order is the mount's own: body armor first, then the saddle, then
    /// the cargo grid, so that shift-clicking a saddle onto an already-armored
    /// horse still saddles it.
    fn quick_move(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        slot_index: usize,
        player: &Player,
    ) -> Option<ItemStack> {
        let clicked = behavior.slots()[slot_index].get_item(guard).clone();
        if clicked.is_empty() {
            return Some(ItemStack::empty());
        }

        let mut stack = clicked.clone();
        let player_start = self.player.start();
        let slot_count = behavior.slots().len();
        let body = self.body.start();
        let saddle = self.saddle.start();

        let moved = if slot_index < player_start {
            behavior.move_item_stack_to(
                guard,
                slot_index,
                &mut stack,
                player_start,
                slot_count,
                FillDirection::Backward,
            )
        } else if accepts(behavior, guard, body, &stack) {
            behavior.move_item_stack_to(
                guard,
                slot_index,
                &mut stack,
                body,
                body + 1,
                FillDirection::Forward,
            )
        } else if accepts(behavior, guard, saddle, &stack) {
            behavior.move_item_stack_to(
                guard,
                slot_index,
                &mut stack,
                saddle,
                saddle + 1,
                FillDirection::Forward,
            )
        } else if self.cargo.is_empty()
            || !behavior.move_item_stack_to(
                guard,
                slot_index,
                &mut stack,
                self.cargo.start(),
                self.cargo.end(),
                FillDirection::Forward,
            )
        {
            // Nothing on the mount wanted it, so it only moves between the
            // player's own two halves. Vanilla returns empty from here without
            // marking the source slot; Steel's move works on a copy, so the
            // copy still has to be written back or the items would double.
            let shuffled = self.shuffle_player_halves(behavior, guard, slot_index, &mut stack);
            if shuffled {
                behavior.update_quick_move_source(guard, slot_index, &stack, &clicked);
            }
            return Some(ItemStack::empty());
        } else {
            true
        };

        if !moved {
            return Some(ItemStack::empty());
        }

        behavior.update_quick_move_source(guard, slot_index, &stack, &clicked);
        if stack.count == clicked.count {
            return Some(ItemStack::empty());
        }
        if let Some(remainder) = behavior.slots()[slot_index].on_take(guard, &stack, player) {
            player.add_item_or_drop_with_guard(guard, remainder);
        }
        Some(clicked)
    }
}

/// Whether an empty equipment slot would take `stack`.
///
/// Vanilla parity: the `getSlot(n).mayPlace(stack) && !getSlot(n).hasItem()`
/// pair the mount menu tests before each armor slot.
fn accepts(
    behavior: &MenuBehavior,
    guard: &ContainerLockGuard,
    slot_index: usize,
    stack: &ItemStack,
) -> bool {
    let slot = &behavior.slots()[slot_index];
    slot.may_place(stack) && !slot.has_item(guard)
}

impl MountKind {
    /// Moves a stack between the player's main inventory and hotbar.
    ///
    /// Vanilla parity: the tail of `AbstractMountInventoryMenu.quickMoveStack`.
    fn shuffle_player_halves(
        &self,
        behavior: &MenuBehavior,
        guard: &mut ContainerLockGuard,
        slot_index: usize,
        stack: &mut ItemStack,
    ) -> bool {
        let (start, end) = if self.hotbar.contains(slot_index) {
            (self.main.start(), self.main.end())
        } else if self.main.contains(slot_index) {
            (self.hotbar.start(), self.hotbar.end())
        } else {
            (self.hotbar.start(), self.main.end())
        };
        behavior.move_item_stack_to(guard, slot_index, stack, start, end, FillDirection::Forward)
    }
}

#[cfg(test)]
mod tests;
