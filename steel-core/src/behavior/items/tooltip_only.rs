//! Items whose whole vanilla class is tooltip text.
//!
//! `SmithingTemplateItem` and `DiscFragmentItem` override nothing but
//! `appendHoverText`, plus -- for the smithing template -- four getters the
//! smithing screen reads for its empty-slot icons and slot descriptions.
//! `ItemStack.getTooltipLines` is only ever called by the client, and the
//! screen runs there too, so a server has no work to do for either class: the
//! item id it already sends is everything the client needs.
//!
//! They are behaviors rather than absences so the parity ledger records that
//! they were checked and found complete, not merely unimplemented.

use steel_macros::item_behavior;

use crate::behavior::ItemBehavior;

/// The netherite upgrade and the nineteen armor trim templates.
///
/// Vanilla parity: `SmithingTemplateItem`, which is entirely client-side --
/// `appendHoverText`, `getBaseSlotDescription`, `getAdditionSlotDescription`,
/// `getBaseSlotEmptyIcons` and `getAdditionalSlotEmptyIcons`. The smithing
/// recipes themselves match on the item, not on this class.
#[item_behavior]
pub struct SmithingTemplateItem;

impl ItemBehavior for SmithingTemplateItem {}

/// The disc fragment.
///
/// Vanilla parity: `DiscFragmentItem`, whose only members are
/// `appendHoverText` and the `getDisplayName` it calls. Crafting five fragments
/// into Disc 5 is an ordinary shaped recipe, not part of this class.
#[item_behavior]
pub struct DiscFragmentItem;

impl ItemBehavior for DiscFragmentItem {}
