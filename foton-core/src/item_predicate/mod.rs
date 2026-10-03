//! Registered item predicates shared by gameplay locks and command matching.

use foton_registry::{
    REGISTRY, RegistryExt as _, data_component_predicate as registered,
    data_components::{
        Component, ComponentData, DataComponentType, PotionContents, vanilla_components,
    },
    item_predicate::{DoubleBounds, IntBounds, ItemPredicate as RegisteredItemPredicate},
    item_stack::ItemStack,
    items::ItemRef,
};
use foton_utils::{DowncastType, Identifier};

pub(crate) trait ItemInstanceView {
    fn item_ref(&self) -> ItemRef;
    fn item_count(&self) -> i32;
    fn effective_value_raw(&self, key: &Identifier) -> Option<&ComponentData>;

    fn component<T: Component + DowncastType>(
        &self,
        component: DataComponentType<T>,
    ) -> Option<&T> {
        self.effective_value_raw(component.key())
            .and_then(ComponentData::downcast_ref::<T>)
    }
}

impl ItemInstanceView for ItemStack {
    fn item_ref(&self) -> ItemRef {
        self.item()
    }

    fn item_count(&self) -> i32 {
        self.count()
    }

    fn effective_value_raw(&self, key: &Identifier) -> Option<&ComponentData> {
        self.get_effective_value_raw(key)
    }
}

impl ItemInstanceView for foton_registry::ItemStackTemplate {
    fn item_ref(&self) -> ItemRef {
        self.item()
    }

    fn item_count(&self) -> i32 {
        self.count()
    }

    fn effective_value_raw(&self, key: &Identifier) -> Option<&ComponentData> {
        self.get_effective_value_raw(key)
    }
}

/// Borrowed registered partial predicates also used by command-specific models.
pub(crate) enum PartialPredicate<'a> {
    Potions(&'a registered::PotionsPredicate),
    Container(&'a registered::ContainerPredicate),
    Bundle(&'a registered::BundlePredicate),
    FireworkExplosion(&'a registered::FireworkExplosionPredicate),
    Fireworks(&'a registered::FireworksPredicate),
    WritableBook(&'a registered::WritableBookPredicate),
    WrittenBook(&'a registered::WrittenBookPredicate),
    Trim(&'a registered::TrimPredicate),
    JukeboxPlayable(&'a registered::JukeboxPlayablePredicate),
    VillagerType(&'a registered::VillagerTypePredicate),
}

impl PartialPredicate<'_> {
    pub(crate) fn matches<S: ItemInstanceView + ?Sized>(&self, stack: &S) -> bool {
        match self {
            Self::Potions(predicate) => stack
                .component(vanilla_components::POTION_CONTENTS)
                .and_then(PotionContents::potion)
                .is_some_and(|potion| predicate.potions().contains(potion.value())),
            Self::Container(predicate) => stack
                .component(vanilla_components::CONTAINER)
                .is_some_and(|contents| {
                    predicate.items().is_none_or(|items| {
                        collection_matches(
                            items,
                            contents.items().iter().filter_map(Option::as_ref),
                            matches_view,
                        )
                    })
                }),
            Self::Bundle(predicate) => stack
                .component(vanilla_components::BUNDLE_CONTENTS)
                .is_some_and(|contents| {
                    predicate.items().is_none_or(|items| {
                        collection_matches(items, contents.items().iter(), matches_view)
                    })
                }),
            Self::FireworkExplosion(predicate) => stack
                .component(vanilla_components::FIREWORK_EXPLOSION)
                .is_some_and(|explosion| firework_matches(predicate.predicate(), explosion)),
            Self::Fireworks(predicate) => stack
                .component(vanilla_components::FIREWORKS)
                .is_some_and(|fireworks| {
                    int_bounds_matches(predicate.flight_duration(), fireworks.flight_duration())
                        && predicate.explosions().is_none_or(|explosions| {
                            collection_matches(
                                explosions,
                                fireworks.explosions().iter(),
                                firework_matches,
                            )
                        })
                }),
            Self::WritableBook(predicate) => stack
                .component(vanilla_components::WRITABLE_BOOK_CONTENT)
                .is_some_and(|book| {
                    predicate.pages().is_none_or(|pages| {
                        collection_matches(pages, book.pages().iter(), |predicate, page| {
                            predicate.contents() == page.raw()
                        })
                    })
                }),
            Self::WrittenBook(predicate) => stack
                .component(vanilla_components::WRITTEN_BOOK_CONTENT)
                .is_some_and(|book| {
                    predicate
                        .author()
                        .is_none_or(|author| author == book.author())
                        && predicate
                            .title()
                            .is_none_or(|title| title == book.title().raw())
                        && int_bounds_matches(predicate.generation(), book.generation())
                        && predicate
                            .resolved()
                            .is_none_or(|resolved| resolved == book.resolved())
                        && predicate.pages().is_none_or(|pages| {
                            collection_matches(pages, book.pages().iter(), |predicate, page| {
                                predicate.contents() == page.raw()
                            })
                        })
                }),
            Self::Trim(predicate) => {
                stack
                    .component(vanilla_components::TRIM)
                    .is_some_and(|trim| {
                        predicate.material().is_none_or(|materials| {
                            trim.material()
                                .as_reference()
                                .is_some_and(|material| materials.contains(material))
                        }) && predicate.pattern().is_none_or(|patterns| {
                            trim.pattern()
                                .as_reference()
                                .is_some_and(|pattern| patterns.contains(pattern))
                        })
                    })
            }
            Self::JukeboxPlayable(predicate) => stack
                .component(vanilla_components::JUKEBOX_PLAYABLE)
                .is_some_and(|playable| {
                    predicate.song().is_none_or(|songs| {
                        playable
                            .song()
                            .as_reference()
                            .is_some_and(|song| songs.contains(song))
                    })
                }),
            Self::VillagerType(predicate) => stack
                .component(vanilla_components::VILLAGER_VARIANT)
                .is_some_and(|villager_type| {
                    predicate.villager_types().contains(villager_type.value())
                }),
        }
    }
}

/// Tests a registered vanilla predicate against a live stack, including empty stacks.
#[must_use]
pub fn matches(predicate: &RegisteredItemPredicate, stack: &ItemStack) -> bool {
    matches_view(predicate, stack)
}

fn int_bounds_matches(bounds: IntBounds, value: i32) -> bool {
    bounds.min().is_none_or(|minimum| value >= minimum)
        && bounds.max().is_none_or(|maximum| value <= maximum)
}

fn double_bounds_matches(bounds: DoubleBounds, value: f64) -> bool {
    bounds.min().is_none_or(|minimum| value >= minimum)
        && bounds.max().is_none_or(|maximum| value <= maximum)
}

fn collection_matches<'a, P, T: 'a>(
    predicate: &registered::CollectionPredicate<P>,
    values: impl IntoIterator<Item = &'a T>,
    matches: impl Fn(&P, &T) -> bool + Copy,
) -> bool {
    let values = values.into_iter().collect::<Vec<_>>();
    predicate.contains().is_none_or(|predicates| {
        predicates
            .iter()
            .all(|predicate| values.iter().any(|value| matches(predicate, value)))
    }) && predicate.counts().is_none_or(|predicates| {
        predicates.iter().all(|predicate| {
            let count = values
                .iter()
                .filter(|value| matches(predicate.test(), value))
                .count();
            i32::try_from(count).is_ok_and(|count| int_bounds_matches(predicate.count(), count))
        })
    }) && predicate.size().is_none_or(|size| {
        i32::try_from(values.len()).is_ok_and(|length| int_bounds_matches(*size, length))
    })
}

fn matches_view<S: ItemInstanceView + ?Sized>(
    predicate: &RegisteredItemPredicate,
    stack: &S,
) -> bool {
    predicate
        .items()
        .is_none_or(|items| items.contains(stack.item_ref()))
        && int_bounds_matches(predicate.count(), stack.item_count())
        && predicate
            .components()
            .exact()
            .values()
            .iter()
            .all(|(entry, expected)| stack.effective_value_raw(&entry.key) == Some(expected))
        && predicate
            .components()
            .partial()
            .iter()
            .all(|predicate| registered_partial_matches(predicate, stack))
}

fn registered_partial_matches<S: ItemInstanceView + ?Sized>(
    predicate: &registered::DataComponentPredicateData,
    stack: &S,
) -> bool {
    if let Some(component) = predicate.any_component() {
        return stack.effective_value_raw(&component.key).is_some();
    }
    macro_rules! match_registered {
        ($type:ty, $variant:ident) => {
            if let Some(value) = predicate.downcast_ref::<$type>() {
                return PartialPredicate::$variant(value).matches(stack);
            }
        };
    }
    match_registered!(registered::PotionsPredicate, Potions);
    match_registered!(registered::ContainerPredicate, Container);
    match_registered!(registered::BundlePredicate, Bundle);
    match_registered!(registered::FireworkExplosionPredicate, FireworkExplosion);
    match_registered!(registered::FireworksPredicate, Fireworks);
    match_registered!(registered::WritableBookPredicate, WritableBook);
    match_registered!(registered::WrittenBookPredicate, WrittenBook);
    match_registered!(registered::TrimPredicate, Trim);
    match_registered!(registered::JukeboxPlayablePredicate, JukeboxPlayable);
    match_registered!(registered::VillagerTypePredicate, VillagerType);

    if let Some(value) = predicate.downcast_ref::<registered::CustomDataPredicate>() {
        return stack
            .component(vanilla_components::CUSTOM_DATA)
            .cloned()
            .unwrap_or_default()
            .matched_by(value.value().tag());
    }
    if let Some(value) = predicate.downcast_ref::<registered::DamagePredicate>() {
        let Some(damage) = stack.component(vanilla_components::DAMAGE).copied() else {
            return false;
        };
        let maximum = stack
            .component(vanilla_components::MAX_DAMAGE)
            .copied()
            .unwrap_or(0);
        return int_bounds_matches(value.durability(), maximum - damage)
            && int_bounds_matches(value.damage(), damage);
    }
    if let Some(value) = predicate.downcast_ref::<registered::EnchantmentsPredicate>() {
        return stack
            .component(vanilla_components::ENCHANTMENTS)
            .is_some_and(|enchantments| {
                value
                    .enchantments()
                    .iter()
                    .all(|predicate| registered_enchantment_matches(predicate, enchantments))
            });
    }
    if let Some(value) = predicate.downcast_ref::<registered::StoredEnchantmentsPredicate>() {
        return stack
            .component(vanilla_components::STORED_ENCHANTMENTS)
            .is_some_and(|enchantments| {
                value
                    .enchantments()
                    .iter()
                    .all(|predicate| registered_enchantment_matches(predicate, enchantments))
            });
    }
    if let Some(value) = predicate.downcast_ref::<registered::AttributeModifiersPredicate>() {
        return stack
            .component(vanilla_components::ATTRIBUTE_MODIFIERS)
            .is_some_and(|modifiers| {
                value.modifiers().is_none_or(|predicate| {
                    collection_matches(
                        predicate,
                        modifiers.modifiers.iter(),
                        registered_attribute_modifier_matches,
                    )
                })
            });
    }
    false
}

fn registered_enchantment_matches(
    predicate: &registered::EnchantmentPredicate,
    enchantments: &vanilla_components::ItemEnchantments,
) -> bool {
    enchantments.iter().any(|(key, level)| {
        predicate.enchantments().is_none_or(|expected| {
            REGISTRY
                .enchantments
                .by_key(key)
                .is_some_and(|enchantment| expected.contains(enchantment))
        }) && predicate
            .levels()
            .min()
            .is_none_or(|minimum| i64::from(*level) >= i64::from(minimum))
            && predicate
                .levels()
                .max()
                .is_none_or(|maximum| i64::from(*level) <= i64::from(maximum))
    })
}

fn registered_attribute_modifier_matches(
    predicate: &registered::AttributeModifierEntryPredicate,
    modifier: &vanilla_components::ItemAttributeModifierEntry,
) -> bool {
    predicate
        .attribute()
        .is_none_or(|attributes| attributes.contains(modifier.attribute))
        && predicate.id().is_none_or(|id| id == &modifier.id)
        && double_bounds_matches(predicate.amount(), modifier.amount)
        && predicate
            .operation()
            .is_none_or(|operation| operation == modifier.operation)
        && predicate.slot().is_none_or(|slot| slot == modifier.slot)
}

fn firework_matches(
    predicate: &registered::FireworkPredicate,
    explosion: &vanilla_components::FireworkExplosion,
) -> bool {
    predicate
        .shape()
        .is_none_or(|shape| shape == explosion.shape())
        && predicate
            .has_twinkle()
            .is_none_or(|twinkle| twinkle == explosion.has_twinkle())
        && predicate
            .has_trail()
            .is_none_or(|trail| trail == explosion.has_trail())
}

#[cfg(test)]
mod tests;
