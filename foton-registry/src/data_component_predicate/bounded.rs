//! Borrowed persistent encoding used by bounded bridge lock serialization.
use foton_utils::serial::nbt_stream::NbtWrite;
use std::io::Result;

use super::{
    AttributeModifierEntryPredicate, AttributeModifiersPredicate, BundlePredicate,
    CollectionCountPredicate, CollectionPredicate, ContainerPredicate, CustomDataPredicate,
    DamagePredicate, EnchantmentPredicate, EnchantmentsPredicate, FireworkExplosionPredicate,
    FireworkPredicate, FireworksPredicate, JukeboxPlayablePredicate, PotionsPredicate,
    StoredEnchantmentsPredicate, TrimPredicate, VillagerTypePredicate, WritableBookPagePredicate,
    WritableBookPredicate, WrittenBookPagePredicate, WrittenBookPredicate,
};
use foton_utils::serial::nbt_encode::{self, NbtEncode, end, field};

macro_rules! compound {
    ($ty:ty, $value:ident, $writer:ident, $depth:ident, $body:block) => {
        impl NbtEncode for $ty {
            fn nbt_id(&self) -> u8 { 10 }
            fn write_nbt_payload(&self, $writer: &mut dyn NbtWrite, $depth: usize) -> Result<()> {
                nbt_encode::check_depth($depth)?;
                let $value = self;
                $body
                end($writer)
            }
        }
    };
}
compound!(DamagePredicate, v, w, d, {
    if !v.durability().is_any() {
        field("durability", &v.durability().as_nbt_tag(), w, d)?;
    }
    if !v.damage().is_any() {
        field("damage", &v.damage().as_nbt_tag(), w, d)?;
    }
});
compound!(EnchantmentPredicate, v, w, d, {
    if let Some(value) = v.enchantments() {
        field("enchantments", value, w, d)?;
    }
    if !v.levels().is_any() {
        field("levels", &v.levels().as_nbt_tag(), w, d)?;
    }
});
macro_rules! list_predicate {
    ($ty:ty) => {
        impl NbtEncode for $ty {
            fn nbt_id(&self) -> u8 {
                9
            }
            fn write_nbt_payload(&self, w: &mut dyn NbtWrite, d: usize) -> Result<()> {
                nbt_encode::list(self.enchantments(), w, d)
            }
        }
    };
}
list_predicate!(EnchantmentsPredicate);
list_predicate!(StoredEnchantmentsPredicate);
macro_rules! holder_predicate {
    ($ty:ty, $method:ident) => {
        impl NbtEncode for $ty {
            fn nbt_id(&self) -> u8 {
                self.$method().nbt_id()
            }
            fn write_nbt_payload(&self, w: &mut dyn NbtWrite, d: usize) -> Result<()> {
                self.$method().write_nbt_payload(w, d)
            }
        }
    };
}
holder_predicate!(PotionsPredicate, potions);
holder_predicate!(VillagerTypePredicate, villager_types);
impl NbtEncode for CustomDataPredicate {
    fn nbt_id(&self) -> u8 {
        self.value().tag().nbt_id()
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        self.value().tag().write_nbt_payload(writer, depth)
    }
}
impl<P: NbtEncode> NbtEncode for CollectionPredicate<P> {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, w: &mut dyn NbtWrite, d: usize) -> Result<()> {
        nbt_encode::check_depth(d)?;
        if let Some(values) = self.contains() {
            field("contains", values, w, d)?;
        }
        if let Some(values) = self.counts() {
            field("count", values, w, d)?;
        }
        if let Some(value) = self.size() {
            field("size", &value.as_nbt_tag(), w, d)?;
        }
        end(w)
    }
}
impl<P: NbtEncode> NbtEncode for CollectionCountPredicate<P> {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, w: &mut dyn NbtWrite, d: usize) -> Result<()> {
        field("test", self.test(), w, d)?;
        field("count", &self.count().as_nbt_tag(), w, d)?;
        end(w)
    }
}
compound!(ContainerPredicate, v, w, d, {
    if let Some(value) = v.items() {
        field("items", value, w, d)?;
    }
});
compound!(BundlePredicate, v, w, d, {
    if let Some(value) = v.items() {
        field("items", value, w, d)?;
    }
});
compound!(WritableBookPredicate, v, w, d, {
    if let Some(value) = v.pages() {
        field("pages", value, w, d)?;
    }
});
impl NbtEncode for WritableBookPagePredicate {
    fn nbt_id(&self) -> u8 {
        8
    }
    fn write_nbt_payload(&self, w: &mut dyn NbtWrite, d: usize) -> Result<()> {
        self.contents().write_nbt_payload(w, d)
    }
}
impl NbtEncode for WrittenBookPagePredicate {
    fn nbt_id(&self) -> u8 {
        self.contents().nbt_id()
    }
    fn write_nbt_payload(&self, w: &mut dyn NbtWrite, d: usize) -> Result<()> {
        self.contents().write_nbt_payload(w, d)
    }
}
compound!(WrittenBookPredicate, v, w, d, {
    if let Some(value) = v.pages() {
        field("pages", value, w, d)?;
    }
    if let Some(value) = v.author() {
        field("author", value, w, d)?;
    }
    if let Some(value) = v.title() {
        field("title", value, w, d)?;
    }
    if !v.generation().is_any() {
        field("generation", &v.generation().as_nbt_tag(), w, d)?;
    }
    if let Some(value) = v.resolved() {
        field("resolved", &value, w, d)?;
    }
});
compound!(AttributeModifierEntryPredicate, v, w, d, {
    if let Some(value) = v.attribute() {
        field("attribute", value, w, d)?;
    }
    if let Some(value) = v.id() {
        field("id", value, w, d)?;
    }
    if !v.amount().is_any() {
        field("amount", &v.amount().as_nbt_tag(), w, d)?;
    }
    if let Some(value) = v.operation() {
        field("operation", &value.name(), w, d)?;
    }
    if let Some(value) = v.slot() {
        field("slot", &value.name(), w, d)?;
    }
});
compound!(AttributeModifiersPredicate, v, w, d, {
    if let Some(value) = v.modifiers() {
        field("modifiers", value, w, d)?;
    }
});
compound!(FireworkPredicate, v, w, d, {
    if let Some(value) = v.shape() {
        field("shape", &value.serialized_name(), w, d)?;
    }
    if let Some(value) = v.has_twinkle() {
        field("has_twinkle", &value, w, d)?;
    }
    if let Some(value) = v.has_trail() {
        field("has_trail", &value, w, d)?;
    }
});
impl NbtEncode for FireworkExplosionPredicate {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, w: &mut dyn NbtWrite, d: usize) -> Result<()> {
        self.predicate().write_nbt_payload(w, d)
    }
}
compound!(FireworksPredicate, v, w, d, {
    if let Some(value) = v.explosions() {
        field("explosions", value, w, d)?;
    }
    if !v.flight_duration().is_any() {
        field("flight_duration", &v.flight_duration().as_nbt_tag(), w, d)?;
    }
});
compound!(TrimPredicate, v, w, d, {
    if let Some(value) = v.material() {
        field("material", value, w, d)?;
    }
    if let Some(value) = v.pattern() {
        field("pattern", value, w, d)?;
    }
});
compound!(JukeboxPlayablePredicate, v, w, d, {
    if let Some(value) = v.song() {
        field("song", value, w, d)?;
    }
});
