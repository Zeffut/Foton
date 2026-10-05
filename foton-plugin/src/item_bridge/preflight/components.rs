use foton_registry::consume_effect::{ApplyStatusEffectsConsumeEffect, ConsumeEffectData};
use foton_registry::data_components::{
    ComponentData,
    components::{
        ArmorTrim, Bees, BlockEntityData, BundleContents, ChargedProjectiles, Consumable,
        CustomData, DeathProtection, EntityData, InstrumentComponent, ItemAttributeModifierDisplay,
        ItemAttributeModifiers, ItemContainerContents, ItemLore, JukeboxPlayable,
        PaintingVariantComponent, PotionContents, ProvidesTrimMaterial, SulfurCubeContent,
        UseRemainder, WrittenBookContent,
    },
};
use foton_registry::item_predicate::{AdventureModePredicate, LockCode};
use foton_registry::mob_effect_instance::MobEffectInstance;
use text_components::TextComponent;

use super::{Node, Walker};

impl<'a> Walker<'a> {
    pub(super) fn component(&mut self, value: &'a ComponentData) {
        self.recursive_items(value);
        self.component_text(value);
        if let Some(value) = value.downcast_ref::<PotionContents>() {
            self.effects(value.custom_effects());
        }
        if let Some(value) = value.downcast_ref::<Consumable>() {
            self.consume_effects(value.on_consume_effects());
        }
        if let Some(value) = value.downcast_ref::<DeathProtection>() {
            self.consume_effects(value.death_effects());
        }
        if let Some(value) = value.downcast_ref::<CustomData>() {
            self.push(Node::Compound(value.as_compound()));
        }
        if let Some(value) = value.downcast_ref::<EntityData>() {
            self.push(Node::Compound(value.data().as_compound()));
        }
        if let Some(value) = value.downcast_ref::<BlockEntityData>() {
            self.push(Node::Compound(value.data().as_compound()));
        }
        if let Some(value) = value.downcast_ref::<Bees>() {
            for bee in value.bees() {
                self.push(Node::Compound(bee.entity_data().data().as_compound()));
            }
        }
        if let Some(value) = value.downcast_ref::<LockCode>() {
            self.push(Node::Matchers(value.predicate().components()));
        }
        if let Some(value) = value.downcast_ref::<AdventureModePredicate>() {
            for predicate in value.predicates() {
                self.push(Node::Matchers(predicate.components()));
                if let Some(nbt) = predicate.nbt() {
                    self.push(Node::Compound(nbt.tag()));
                }
            }
        }
    }

    fn effects(&mut self, effects: &'a [MobEffectInstance]) {
        for effect in effects {
            if let Some(hidden) = effect.hidden_effect() {
                self.push(Node::Effect(hidden));
            }
        }
    }

    fn consume_effects(&mut self, effects: &'a [ConsumeEffectData]) {
        for effect in effects {
            if let Some(value) = effect.downcast_ref::<ApplyStatusEffectsConsumeEffect>() {
                self.effects(value.effects());
            }
        }
    }

    fn recursive_items(&mut self, value: &'a ComponentData) {
        if let Some(value) = value.downcast_ref::<UseRemainder>() {
            self.push(Node::Template(value.convert_into()));
        }
        if let Some(value) = value.downcast_ref::<SulfurCubeContent>() {
            self.push(Node::Template(value.absorbed_block_item_stack()));
        }
        if let Some(value) = value.downcast_ref::<ChargedProjectiles>() {
            for item in value.items() {
                self.push(Node::Template(item));
            }
        }
        if let Some(value) = value.downcast_ref::<BundleContents>() {
            for item in value.items() {
                self.push(Node::Template(item));
            }
        }
        if let Some(value) = value.downcast_ref::<ItemContainerContents>() {
            for item in value.items().iter().flatten() {
                self.push(Node::Template(item));
            }
        }
    }

    fn component_text(&mut self, value: &'a ComponentData) {
        if let Some(value) = value.downcast_ref::<TextComponent>() {
            self.push(Node::Text(value));
        }
        if let Some(value) = value.downcast_ref::<ItemLore>() {
            for line in value.lines().iter().chain(value.styled_lines()) {
                self.push(Node::Text(line));
            }
        }
        if let Some(value) = value.downcast_ref::<WrittenBookContent>() {
            for page in value.pages() {
                self.push(Node::Text(page.raw()));
                if let Some(filtered) = page.filtered() {
                    self.push(Node::Text(filtered));
                }
            }
        }
        if let Some(value) = value.downcast_ref::<ItemAttributeModifiers>() {
            for entry in &value.modifiers {
                if let ItemAttributeModifierDisplay::OverrideText(text) = &entry.display {
                    self.push(Node::Text(text));
                }
            }
        }
        self.holder_text(value);
    }

    fn holder_text(&mut self, value: &'a ComponentData) {
        // Registry references are frozen and are not cloned. Direct values are owned.
        if let Some(value) = value.downcast_ref::<ArmorTrim>() {
            if let Some(material) = value.material().as_direct() {
                self.push(Node::Text(material.description()));
            }
            if let Some(pattern) = value.pattern().as_direct() {
                self.push(Node::Text(pattern.description()));
            }
        }
        if let Some(value) = value.downcast_ref::<ProvidesTrimMaterial>()
            && let Some(material) = value.material().as_direct()
        {
            self.push(Node::Text(material.description()));
        }
        if let Some(value) = value.downcast_ref::<InstrumentComponent>()
            && let Some(instrument) = value.instrument().as_direct()
        {
            self.push(Node::Text(instrument.description()));
        }
        if let Some(value) = value.downcast_ref::<JukeboxPlayable>()
            && let Some(song) = value.song().as_direct()
        {
            self.push(Node::Text(&song.description));
        }
        if let Some(value) = value.downcast_ref::<PaintingVariantComponent>()
            && let Some(painting) = value.variant().as_direct()
        {
            for text in painting.title.iter().chain(painting.author.iter()) {
                self.push(Node::Text(text));
            }
        }
    }
}
