//! Vanilla advancement definitions and the per-player progress over them.
//!
//! Vanilla parity: `net.minecraft.advancements`. The definitions are generated
//! from the built-in datapack by `foton-registry/build/advancements.rs`, so
//! everything here is plain data plus the codecs the protocol needs. The tree,
//! the per-player bookkeeping and the triggers live in `foton-core`.

pub mod predicate;
pub mod progress;
pub mod registry;
pub mod trigger;

use std::str::FromStr as _;
use std::sync::OnceLock;

use foton_utils::Identifier;
use foton_utils::translations;
use text_components::translation::Translation;
use text_components::{TextComponent, format::Color};

use crate::item_stack_template::ItemStackTemplate;
use crate::items::ItemRef;
use crate::{REGISTRY, RegistryExt as _};

pub use progress::{AdvancementProgress, CriterionProgress};
pub use registry::{AdvancementRef, AdvancementRegistry};
pub use trigger::TriggerInstance;

/// The frame an advancement's icon is drawn in.
///
/// Vanilla parity: `AdvancementType`. The wire form is the ordinal, so the
/// declaration order is protocol-observable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AdvancementType {
    /// An ordinary advancement.
    #[default]
    Task = 0,
    /// A challenge, drawn in a spiked frame and announced in purple.
    Challenge = 1,
    /// A goal, drawn in a rounded frame.
    Goal = 2,
}

impl AdvancementType {
    /// The name this frame is written under in a datapack.
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Challenge => "challenge",
            Self::Goal => "goal",
        }
    }

    /// The color the chat announcement is written in.
    ///
    /// Vanilla parity: `AdvancementType.getChatColor`.
    #[must_use]
    pub const fn chat_color(self) -> Color {
        match self {
            Self::Task | Self::Goal => Color::Green,
            Self::Challenge => Color::DarkPurple,
        }
    }

    /// The chat announcement for this frame, which takes the player's name
    /// and the decorated advancement name.
    ///
    /// Vanilla parity: `AdvancementType.createAnnouncement`, which builds
    /// `chat.type.advancement.<name>`.
    #[must_use]
    pub const fn announcement(self) -> &'static Translation<2> {
        match self {
            Self::Task => &translations::CHAT_TYPE_ADVANCEMENT_TASK,
            Self::Challenge => &translations::CHAT_TYPE_ADVANCEMENT_CHALLENGE,
            Self::Goal => &translations::CHAT_TYPE_ADVANCEMENT_GOAL,
        }
    }
}

/// The item drawn in an advancement's frame.
///
/// Vanilla parity: the `icon` field of `DisplayInfo`, an `ItemStackTemplate`.
/// A template carries a data component patch, which cannot be built in a
/// `const`, so the datapack's own `components` block is kept verbatim as SNBT
/// -- `ItemStackTemplate`'s NBT codec has the same shape as its JSON one --
/// and decoded once, the first time a packet needs it. The generated items are
/// `LazyLock`s, which cannot be dereferenced in a `const` either, so the icon
/// holds its item's key and resolves that on first use too.
#[derive(Debug)]
pub struct AdvancementIcon {
    item_id: &'static str,
    components_snbt: &'static str,
    template: OnceLock<ItemStackTemplate>,
}

impl AdvancementIcon {
    /// Wraps the datapack's icon. `components_snbt` is empty for a bare item.
    #[must_use]
    pub const fn new(item_id: &'static str, components_snbt: &'static str) -> Self {
        Self {
            item_id,
            components_snbt,
            template: OnceLock::new(),
        }
    }

    /// The registry key of the item the icon shows.
    #[must_use]
    pub const fn item_id(&self) -> &'static str {
        self.item_id
    }

    /// The `components` block the icon was generated from, empty for a bare
    /// item.
    #[must_use]
    pub const fn components_snbt(&self) -> &'static str {
        self.components_snbt
    }

    /// The icon as an item stack template, resolved and decoded on first use.
    ///
    /// An icon whose component patch will not decode falls back to the bare
    /// item: a tooltip Foton cannot model is not worth refusing to draw the
    /// advancement screen over. `advancement_icons_decode_with_their_components`
    /// is the test that keeps that fallback from going unnoticed.
    ///
    /// # Panics
    /// If the generated item key is not in the item registry. Both come from
    /// the same extracted vanilla data, so that is a build inconsistency
    /// rather than a runtime condition.
    pub fn template(&self) -> &ItemStackTemplate {
        self.template.get_or_init(|| {
            let item = self.resolve_item();
            if self.components_snbt.is_empty() {
                return ItemStackTemplate::new(item);
            }
            decode_icon(item, self.components_snbt).unwrap_or_else(|| {
                log::error!(
                    "advancement icon {} has a component patch Foton could not decode: {}",
                    self.item_id,
                    self.components_snbt
                );
                ItemStackTemplate::new(item)
            })
        })
    }

    fn resolve_item(&self) -> ItemRef {
        let key = Identifier::from_str(self.item_id).unwrap_or_else(|_| {
            panic!(
                "generated advancement icon id {} is not an identifier",
                self.item_id
            )
        });
        REGISTRY.items.by_key(&key).unwrap_or_else(|| {
            panic!(
                "generated advancement icon {} is not a registered item",
                self.item_id
            )
        })
    }
}

fn decode_icon(item: ItemRef, components_snbt: &str) -> Option<ItemStackTemplate> {
    use std::io::Cursor;

    use foton_utils::nbt::parse_snbt_compound;
    use simdnbt::FromNbtTag as _;
    use simdnbt::owned::NbtTag;

    use crate::data_components::DataComponentPatch;

    let compound = parse_snbt_compound(components_snbt).ok()?;
    // The patch decodes from a borrowed tag, so the owned compound takes the
    // usual trip through the binary form to become one.
    let mut bytes = Vec::new();
    NbtTag::Compound(compound).write(&mut bytes);
    let borrowed = simdnbt::borrow::read_tag(&mut Cursor::new(bytes.as_slice())).ok()?;
    let patch = DataComponentPatch::from_nbt_tag(borrowed.as_tag())?;
    ItemStackTemplate::try_with_count_and_patch(item, 1, patch).ok()
}

/// Everything the advancement screen needs to draw one advancement.
///
/// Vanilla parity: `DisplayInfo`. Vanilla mutates `x`/`y` at load time from
/// `TreeNodePosition`; Foton's advancement set is fixed at build time, so the
/// same layout runs in the build script and the coordinates are constants.
#[derive(Debug)]
pub struct DisplayInfo {
    /// The advancement's name.
    pub title: TextComponent,
    /// The line under the name.
    pub description: TextComponent,
    /// The item in the frame.
    pub icon: AdvancementIcon,
    /// The tab background texture, present only on a root.
    pub background: Option<Identifier>,
    /// The frame the icon is drawn in.
    pub advancement_type: AdvancementType,
    /// Whether earning it pops a toast.
    pub show_toast: bool,
    /// Whether earning it is announced in chat.
    ///
    /// Server-side only: it is deliberately not on the wire, because the
    /// server is the one that broadcasts the message.
    pub announce_chat: bool,
    /// Whether it stays hidden until earned.
    pub hidden: bool,
    /// The column, which is the depth in the tree.
    pub x: f32,
    /// The row the layout put it on.
    pub y: f32,
}

/// What earning an advancement hands out.
///
/// Vanilla parity: `AdvancementRewards`. Never sent to the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancementRewards {
    /// Experience points granted.
    pub experience: i32,
    /// Loot tables rolled into the player's inventory.
    pub loot: &'static [Identifier],
    /// Recipes unlocked in the recipe book.
    pub recipes: &'static [Identifier],
    /// A function run as the server at gamemaster level.
    pub function: Option<Identifier>,
}

impl AdvancementRewards {
    /// Rewards that hand out nothing.
    pub const EMPTY: Self = Self {
        experience: 0,
        loot: &[],
        recipes: &[],
        function: None,
    };

    /// Whether nothing at all is granted.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.experience == 0
            && self.loot.is_empty()
            && self.recipes.is_empty()
            && self.function.is_none()
    }
}

/// One named condition of an advancement.
///
/// Vanilla parity: an entry of `Advancement.criteria`, which is a `Criterion`
/// pairing a trigger with its instance.
#[derive(Debug, Clone, PartialEq)]
pub struct Criterion {
    /// The name the advancement refers to this criterion by.
    pub name: &'static str,
    /// The trigger and the conditions it must satisfy.
    pub trigger: TriggerInstance,
}

/// Which criteria have to be met, as an AND of ORs.
///
/// Vanilla parity: `AdvancementRequirements`. The outer list is a conjunction,
/// each inner list a disjunction. An empty outer list is never satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvancementRequirements {
    /// The requirement groups.
    pub groups: &'static [&'static [&'static str]],
}

impl AdvancementRequirements {
    /// Requirements that can never be met.
    pub const EMPTY: Self = Self { groups: &[] };

    /// The number of groups, which is the denominator of the client's
    /// "x/y" progress text.
    ///
    /// Vanilla parity: `AdvancementRequirements.size`.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.groups.len()
    }

    /// Whether every group has at least one criterion `predicate` accepts.
    ///
    /// Vanilla parity: `AdvancementRequirements.test`, including its refusal
    /// to ever pass an empty requirement list.
    pub fn test(&self, mut predicate: impl FnMut(&str) -> bool) -> bool {
        if self.groups.is_empty() {
            return false;
        }
        self.groups
            .iter()
            .all(|group| group.iter().any(|name| predicate(name)))
    }

    /// How many groups have at least one criterion `predicate` accepts.
    ///
    /// Vanilla parity: `AdvancementRequirements.count`.
    pub fn count(&self, mut predicate: impl FnMut(&str) -> bool) -> usize {
        self.groups
            .iter()
            .filter(|group| group.iter().any(|name| predicate(name)))
            .count()
    }

    /// Every criterion name mentioned by any group, without duplicates.
    ///
    /// Vanilla parity: `AdvancementRequirements.names`.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.groups.iter().flat_map(|group| group.iter().copied())
    }
}

/// One advancement, as the built-in datapack defines it.
///
/// Vanilla parity: `Advancement` together with the `AdvancementHolder` that
/// gives it its id. Foton keeps the id on the value, the way every other
/// registry entry here does.
#[derive(Debug)]
pub struct Advancement {
    /// The advancement's registry key.
    pub key: Identifier,
    /// The advancement this one hangs off, absent on a root.
    pub parent: Option<Identifier>,
    /// What the advancement screen shows, absent for an invisible advancement.
    pub display: Option<DisplayInfo>,
    /// What earning it hands out.
    pub rewards: AdvancementRewards,
    /// Its named conditions.
    pub criteria: &'static [Criterion],
    /// Which of them have to be met.
    pub requirements: AdvancementRequirements,
    /// Whether vanilla reports it to its telemetry service. Foton never does;
    /// the flag is kept because the client receives it.
    pub sends_telemetry_event: bool,
}

impl Advancement {
    /// Whether this advancement is the root of its tab.
    ///
    /// Vanilla parity: `Advancement.isRoot`, which is exactly "has no parent".
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    /// Looks a criterion up by name.
    #[must_use]
    pub fn criterion(&self, name: &str) -> Option<&'static Criterion> {
        // The slice is `&'static` on every generated advancement, so the
        // borrow can be handed back with that lifetime.
        self.criteria
            .iter()
            .find(|criterion| criterion.name == name)
    }
}

#[cfg(test)]
mod tests;
