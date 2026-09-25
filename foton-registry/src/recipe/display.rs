//! What the recipe book shows of a recipe.
//!
//! Vanilla parity: `RecipeDisplay`, `SlotDisplay` and `RecipeDisplayEntry`.
//! Since 1.21.2 the client never sees a recipe itself: the server describes
//! each one as a display -- the grid, the result, the station -- and the client
//! draws and matches against that picture alone. The encoding is the recipe
//! book's whole protocol surface, so each type here mirrors its vanilla stream
//! codec field for field, and every registry id comes from the generated
//! [`crate::vanilla_recipe_book_registries`].
//!
//! Only the display shapes Foton's recipes can produce exist here. Vanilla also
//! has `with_any_potion`, `only_with_component`, `dyed` and `smithing_trim`
//! slots, which belong to the imbue, dye and trim recipes Foton does not load.

use std::io::{Error, Result, Write};

use foton_utils::Identifier;
use foton_utils::codec::VarInt;
use foton_utils::serial::WriteTo;

use crate::data_components::DataComponentPatch;
use crate::item_stack_template::ItemStackTemplate;
use crate::items::ItemRef;
use crate::vanilla_recipe_book_registries::{
    recipe_book_category, recipe_display_type, slot_display_type,
};
use crate::{RegistryEntry as _, vanilla_items};

use super::{
    CookingCategory, CraftingCategory, Ingredient, RecipeResult, ShapedRecipe, ShapelessRecipe,
    SmeltingRecipe, SmithingTransformRecipe, StonecuttingRecipe,
};

/// Writes a VarInt-prefixed list, the shape of vanilla's `ByteBufCodecs.list()`.
fn write_list<T>(
    items: &[T],
    writer: &mut impl Write,
    mut write: impl FnMut(&T, &mut dyn Write) -> Result<()>,
) -> Result<()> {
    let length =
        i32::try_from(items.len()).map_err(|_| Error::other("list too long for a VarInt"))?;
    VarInt(length).write(writer)?;
    for item in items {
        write(item, writer)?;
    }
    Ok(())
}

/// Writes an item's network id.
///
/// Vanilla parity: `Item.STREAM_CODEC`, a plain registry id.
fn write_item(item: ItemRef, writer: &mut impl Write) -> Result<()> {
    let id = i32::try_from(item.id()).map_err(|_| Error::other("item id too large"))?;
    VarInt(id).write(writer)
}

/// The tab a recipe is filed under in the recipe book.
///
/// Vanilla parity: `RecipeBookCategories`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeBookCategory {
    CraftingBuildingBlocks,
    CraftingRedstone,
    CraftingEquipment,
    CraftingMisc,
    FurnaceFood,
    FurnaceBlocks,
    FurnaceMisc,
    BlastFurnaceBlocks,
    BlastFurnaceMisc,
    SmokerFood,
    Stonecutter,
    Smithing,
    Campfire,
}

impl RecipeBookCategory {
    /// The category's id in the `recipe_book_category` registry.
    #[must_use]
    pub const fn network_id(self) -> i32 {
        match self {
            Self::CraftingBuildingBlocks => recipe_book_category::CRAFTING_BUILDING_BLOCKS,
            Self::CraftingRedstone => recipe_book_category::CRAFTING_REDSTONE,
            Self::CraftingEquipment => recipe_book_category::CRAFTING_EQUIPMENT,
            Self::CraftingMisc => recipe_book_category::CRAFTING_MISC,
            Self::FurnaceFood => recipe_book_category::FURNACE_FOOD,
            Self::FurnaceBlocks => recipe_book_category::FURNACE_BLOCKS,
            Self::FurnaceMisc => recipe_book_category::FURNACE_MISC,
            Self::BlastFurnaceBlocks => recipe_book_category::BLAST_FURNACE_BLOCKS,
            Self::BlastFurnaceMisc => recipe_book_category::BLAST_FURNACE_MISC,
            Self::SmokerFood => recipe_book_category::SMOKER_FOOD,
            Self::Stonecutter => recipe_book_category::STONECUTTER,
            Self::Smithing => recipe_book_category::SMITHING,
            Self::Campfire => recipe_book_category::CAMPFIRE,
        }
    }

    /// Vanilla parity: `CraftingRecipe.recipeBookCategory`.
    #[must_use]
    pub const fn crafting(category: CraftingCategory) -> Self {
        match category {
            CraftingCategory::Building => Self::CraftingBuildingBlocks,
            CraftingCategory::Equipment => Self::CraftingEquipment,
            CraftingCategory::Redstone => Self::CraftingRedstone,
            CraftingCategory::Misc => Self::CraftingMisc,
        }
    }
}

/// Which block a cooking recipe runs in.
///
/// Vanilla parity: the four `AbstractCookingRecipe` subclasses, which differ
/// only in the icon they show and the tab they file themselves under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookingStation {
    Furnace,
    BlastFurnace,
    Smoker,
    Campfire,
}

impl CookingStation {
    /// Vanilla parity: `furnaceIcon`.
    fn icon(self) -> ItemRef {
        match self {
            Self::Furnace => &vanilla_items::FURNACE,
            Self::BlastFurnace => &vanilla_items::BLAST_FURNACE,
            Self::Smoker => &vanilla_items::SMOKER,
            Self::Campfire => &vanilla_items::CAMPFIRE,
        }
    }

    /// Vanilla parity: each subclass's `recipeBookCategory`. A blast furnace
    /// files food under misc, and a smoker and a campfire ignore the category
    /// altogether.
    const fn category(self, category: CookingCategory) -> RecipeBookCategory {
        match (self, category) {
            (Self::Furnace, CookingCategory::Blocks) => RecipeBookCategory::FurnaceBlocks,
            (Self::Furnace, CookingCategory::Food) => RecipeBookCategory::FurnaceFood,
            (Self::Furnace, CookingCategory::Misc) => RecipeBookCategory::FurnaceMisc,
            (Self::BlastFurnace, CookingCategory::Blocks) => RecipeBookCategory::BlastFurnaceBlocks,
            (Self::BlastFurnace, CookingCategory::Food | CookingCategory::Misc) => {
                RecipeBookCategory::BlastFurnaceMisc
            }
            (Self::Smoker, _) => RecipeBookCategory::SmokerFood,
            (Self::Campfire, _) => RecipeBookCategory::Campfire,
        }
    }
}

/// What one slot of a recipe display shows.
///
/// Vanilla parity: `SlotDisplay`, restricted to the variants Foton's recipes
/// produce.
#[derive(Debug, Clone, PartialEq)]
pub enum SlotDisplay {
    Empty,
    AnyFuel,
    Item(ItemRef),
    ItemStack(ItemStackTemplate),
    Tag(Identifier),
    WithRemainder {
        input: Box<Self>,
        remainder: Box<Self>,
    },
    Composite(Vec<Self>),
}

impl SlotDisplay {
    /// Vanilla parity: `Ingredient.display` through
    /// `Ingredient.optionalIngredientToDisplay`, `Empty` standing for the
    /// absent ingredient. A tag stays a tag; a list of items, even a list of
    /// one, becomes a composite, each item carrying its crafting remainder.
    #[must_use]
    pub fn of_ingredient(ingredient: &Ingredient) -> Self {
        match ingredient {
            Ingredient::Empty => Self::Empty,
            Ingredient::Tag(tag) => Self::Tag(tag.clone()),
            Ingredient::Item(item) => Self::Composite(vec![Self::for_single_item(*item)]),
            Ingredient::Choice(items) => {
                Self::Composite(items.iter().map(|item| Self::for_single_item(*item)).collect())
            }
        }
    }

    /// Vanilla parity: `Ingredient.displayForSingleItem`.
    fn for_single_item(item: ItemRef) -> Self {
        let display = Self::Item(item);
        let remainder = item.get_crafting_remainder();
        if remainder.is_empty() {
            return display;
        }
        Self::WithRemainder {
            input: Box::new(display),
            remainder: Box::new(Self::ItemStack(ItemStackTemplate::new(remainder.item))),
        }
    }

    /// Vanilla parity: `new SlotDisplay.ItemStackSlotDisplay(result)`. `None`
    /// when the result is not a valid `ItemStackTemplate`, which a plugin
    /// recipe asking for more than 99 items would not be.
    fn of_result(result: &RecipeResult) -> Option<Self> {
        ItemStackTemplate::try_with_count_and_patch(
            result.item,
            result.count,
            DataComponentPatch::new(),
        )
        .ok()
        .map(Self::ItemStack)
    }

    const fn type_id(&self) -> i32 {
        match self {
            Self::Empty => slot_display_type::EMPTY,
            Self::AnyFuel => slot_display_type::ANY_FUEL,
            Self::Item(_) => slot_display_type::ITEM,
            Self::ItemStack(_) => slot_display_type::ITEM_STACK,
            Self::Tag(_) => slot_display_type::TAG,
            Self::WithRemainder { .. } => slot_display_type::WITH_REMAINDER,
            Self::Composite(_) => slot_display_type::COMPOSITE,
        }
    }

    fn write_dyn(&self, writer: &mut dyn Write) -> Result<()> {
        let mut writer = writer;
        self.write(&mut writer)
    }
}

impl WriteTo for SlotDisplay {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.type_id()).write(writer)?;
        match self {
            Self::Empty | Self::AnyFuel => Ok(()),
            Self::Item(item) => write_item(*item, writer),
            Self::ItemStack(stack) => stack.write(writer),
            Self::Tag(tag) => tag.write(writer),
            Self::WithRemainder { input, remainder } => {
                input.write(writer)?;
                remainder.write(writer)
            }
            Self::Composite(contents) => write_list(contents, writer, Self::write_dyn),
        }
    }
}

/// How a recipe is drawn in the recipe book.
///
/// Vanilla parity: the five `RecipeDisplay` types.
#[derive(Debug, Clone, PartialEq)]
pub enum RecipeDisplay {
    CraftingShapeless {
        ingredients: Vec<SlotDisplay>,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
    CraftingShaped {
        width: i32,
        height: i32,
        ingredients: Vec<SlotDisplay>,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
    Furnace {
        ingredient: SlotDisplay,
        fuel: SlotDisplay,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
        duration: i32,
        experience: f32,
    },
    Stonecutter {
        input: SlotDisplay,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
    Smithing {
        template: SlotDisplay,
        base: SlotDisplay,
        addition: SlotDisplay,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
}

impl RecipeDisplay {
    const fn type_id(&self) -> i32 {
        match self {
            Self::CraftingShapeless { .. } => recipe_display_type::CRAFTING_SHAPELESS,
            Self::CraftingShaped { .. } => recipe_display_type::CRAFTING_SHAPED,
            Self::Furnace { .. } => recipe_display_type::FURNACE,
            Self::Stonecutter { .. } => recipe_display_type::STONECUTTER,
            Self::Smithing { .. } => recipe_display_type::SMITHING,
        }
    }
}

impl WriteTo for RecipeDisplay {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.type_id()).write(writer)?;
        match self {
            Self::CraftingShapeless {
                ingredients,
                result,
                crafting_station,
            } => {
                write_list(ingredients, writer, SlotDisplay::write_dyn)?;
                result.write(writer)?;
                crafting_station.write(writer)
            }
            Self::CraftingShaped {
                width,
                height,
                ingredients,
                result,
                crafting_station,
            } => {
                VarInt(*width).write(writer)?;
                VarInt(*height).write(writer)?;
                write_list(ingredients, writer, SlotDisplay::write_dyn)?;
                result.write(writer)?;
                crafting_station.write(writer)
            }
            Self::Furnace {
                ingredient,
                fuel,
                result,
                crafting_station,
                duration,
                experience,
            } => {
                ingredient.write(writer)?;
                fuel.write(writer)?;
                result.write(writer)?;
                crafting_station.write(writer)?;
                VarInt(*duration).write(writer)?;
                experience.write(writer)
            }
            Self::Stonecutter {
                input,
                result,
                crafting_station,
            } => {
                input.write(writer)?;
                result.write(writer)?;
                crafting_station.write(writer)
            }
            Self::Smithing {
                template,
                base,
                addition,
                result,
                crafting_station,
            } => {
                template.write(writer)?;
                base.write(writer)?;
                addition.write(writer)?;
                result.write(writer)?;
                crafting_station.write(writer)
            }
        }
    }
}

/// Writes an ingredient's item set.
///
/// Vanilla parity: `Ingredient.CONTENTS_STREAM_CODEC`, which is
/// `ByteBufCodecs.holderSet(ITEM)`: `0` then the tag's name for a tag, or the
/// item count plus one followed by the item ids.
fn write_ingredient_contents(ingredient: &Ingredient, writer: &mut dyn Write) -> Result<()> {
    let mut writer = writer;
    let items: &[ItemRef] = match ingredient {
        Ingredient::Tag(tag) => {
            VarInt(0).write(&mut writer)?;
            return tag.write(&mut writer);
        }
        Ingredient::Empty => &[],
        Ingredient::Item(item) => std::slice::from_ref(item),
        Ingredient::Choice(items) => items,
    };
    let count = i32::try_from(items.len()).map_err(|_| Error::other("too many items"))?;
    VarInt(count + 1).write(&mut writer)?;
    for item in items {
        write_item(*item, &mut writer)?;
    }
    Ok(())
}

/// One recipe display as the recipe book knows it.
///
/// Vanilla parity: `RecipeDisplayEntry`. `id` is the server's handle for the
/// display, which the client sends back when it marks a recipe seen or asks
/// for it to be placed.
#[derive(Debug, Clone)]
pub struct RecipeDisplayEntry {
    pub id: i32,
    pub display: RecipeDisplay,
    /// The recipe's group, interned to a number; `None` for an ungrouped one.
    pub group: Option<i32>,
    pub category: RecipeBookCategory,
    /// The ingredients the client checks its inventory against, or `None`
    /// for a special recipe.
    pub crafting_requirements: Option<Vec<Ingredient>>,
}

impl WriteTo for RecipeDisplayEntry {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.id).write(writer)?;
        self.display.write(writer)?;
        // Vanilla parity: `ByteBufCodecs.OPTIONAL_VAR_INT`, zero for none and
        // the value plus one otherwise -- not a boolean-prefixed option.
        VarInt(self.group.map_or(0, |group| group + 1)).write(writer)?;
        VarInt(self.category.network_id()).write(writer)?;
        match &self.crafting_requirements {
            None => false.write(writer),
            Some(requirements) => {
                true.write(writer)?;
                write_list(requirements, writer, write_ingredient_contents)
            }
        }
    }
}

/// What the recipe book needs of a recipe, before the index gives its display
/// an id and its group a number.
pub(crate) struct DisplaySource<'a> {
    pub show_notification: bool,
    pub group: &'a str,
    pub category: RecipeBookCategory,
    pub display: Option<RecipeDisplay>,
    /// Vanilla parity: `placementInfo().ingredients()` -- every ingredient
    /// that fills a slot, in slot order.
    pub crafting_requirements: Vec<Ingredient>,
}

/// The ingredients that fill a slot, dropping the empty ones.
///
/// Vanilla parity: `PlacementInfo.createFromOptionals`.
fn present(ingredients: impl IntoIterator<Item = Ingredient>) -> Vec<Ingredient> {
    ingredients
        .into_iter()
        .filter(|ingredient| !ingredient.is_empty())
        .collect()
}

impl ShapedRecipe {
    /// Vanilla parity: `ShapedRecipe.display`.
    pub(crate) fn display_source(&self) -> DisplaySource<'_> {
        let display = SlotDisplay::of_result(&self.result).and_then(|result| {
            Some(RecipeDisplay::CraftingShaped {
                width: i32::try_from(self.width).ok()?,
                height: i32::try_from(self.height).ok()?,
                ingredients: self.pattern.iter().map(SlotDisplay::of_ingredient).collect(),
                result,
                crafting_station: SlotDisplay::Item(&vanilla_items::CRAFTING_TABLE),
            })
        });
        DisplaySource {
            show_notification: self.show_notification,
            group: &self.group,
            category: RecipeBookCategory::crafting(self.category),
            display,
            crafting_requirements: present(self.pattern.iter().cloned()),
        }
    }
}

impl ShapelessRecipe {
    /// Vanilla parity: `ShapelessRecipe.display`.
    pub(crate) fn display_source(&self) -> DisplaySource<'_> {
        let display =
            SlotDisplay::of_result(&self.result).map(|result| RecipeDisplay::CraftingShapeless {
                ingredients: self
                    .ingredients
                    .iter()
                    .map(SlotDisplay::of_ingredient)
                    .collect(),
                result,
                crafting_station: SlotDisplay::Item(&vanilla_items::CRAFTING_TABLE),
            });
        DisplaySource {
            show_notification: self.show_notification,
            group: &self.group,
            category: RecipeBookCategory::crafting(self.category),
            display,
            crafting_requirements: self.ingredients.to_vec(),
        }
    }
}

impl SmeltingRecipe {
    /// Vanilla parity: `AbstractCookingRecipe.display`.
    pub(crate) fn display_source(&self, station: CookingStation) -> DisplaySource<'_> {
        let display = SlotDisplay::of_result(&self.result).map(|result| RecipeDisplay::Furnace {
            ingredient: SlotDisplay::of_ingredient(&self.ingredient),
            fuel: SlotDisplay::AnyFuel,
            result,
            crafting_station: SlotDisplay::Item(station.icon()),
            duration: self.cooking_time,
            experience: self.experience,
        });
        DisplaySource {
            show_notification: self.show_notification,
            group: &self.group,
            category: station.category(self.category),
            display,
            crafting_requirements: vec![self.ingredient.clone()],
        }
    }
}

impl StonecuttingRecipe {
    /// Vanilla parity: `StonecutterRecipe.display`, whose group is always empty.
    pub(crate) fn display_source(&self) -> DisplaySource<'_> {
        let display =
            SlotDisplay::of_result(&self.result).map(|result| RecipeDisplay::Stonecutter {
                input: SlotDisplay::of_ingredient(&self.ingredient),
                result,
                crafting_station: SlotDisplay::Item(&vanilla_items::STONECUTTER),
            });
        DisplaySource {
            show_notification: self.show_notification,
            group: "",
            category: RecipeBookCategory::Stonecutter,
            display,
            crafting_requirements: vec![self.ingredient.clone()],
        }
    }
}

impl SmithingTransformRecipe {
    /// Vanilla parity: `SmithingTransformRecipe.display`, whose group is
    /// always empty.
    pub(crate) fn display_source(&self) -> DisplaySource<'_> {
        let display = SlotDisplay::of_result(&self.result).map(|result| RecipeDisplay::Smithing {
            template: SlotDisplay::of_ingredient(&self.template),
            base: SlotDisplay::of_ingredient(&self.base),
            addition: SlotDisplay::of_ingredient(&self.addition),
            result,
            crafting_station: SlotDisplay::Item(&vanilla_items::SMITHING_TABLE),
        });
        DisplaySource {
            show_notification: self.show_notification,
            group: "",
            category: RecipeBookCategory::Smithing,
            display,
            crafting_requirements: present([
                self.template.clone(),
                self.base.clone(),
                self.addition.clone(),
            ]),
        }
    }
}
