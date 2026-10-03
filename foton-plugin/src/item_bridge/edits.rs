//! Validated explicit component edits; absent keys mean unchanged.

use foton_registry::data_components::ComponentData;
use foton_registry::{REGISTRY, RegistryExt};
use foton_utils::Identifier;
use rustc_hash::FxHashSet;

use super::{ItemBridgeError, preflight, snapshot::Candidate};

pub(crate) enum Operation {
    Set(ComponentData),
    Remove,
    Reset,
}

pub(crate) struct Edit {
    pub key: Identifier,
    pub operation: Operation,
}

/// Holds only validated edits. Registry identity is immutable for this process epoch.
pub(crate) struct Edits(Vec<Edit>);

impl Edits {
    pub(crate) fn new(edits: Vec<Edit>) -> Result<Self, ItemBridgeError> {
        if edits.len() > 4096 {
            return Err(ItemBridgeError::InvalidEdit("too many component edits"));
        }
        let mut keys = FxHashSet::default();
        for edit in &edits {
            if !keys.insert(&edit.key) {
                return Err(ItemBridgeError::InvalidEdit("duplicate component edit"));
            }
            let entry = REGISTRY
                .data_components
                .by_key(&edit.key)
                .ok_or(ItemBridgeError::InvalidEdit("unregistered component"))?;
            if let Operation::Set(value) = &edit.operation {
                if !entry.validates(value) {
                    return Err(ItemBridgeError::InvalidEdit("incompatible component value"));
                }
                preflight::check_component(value)?;
            }
        }
        Ok(Self(edits))
    }

    /// Failure discards the detached candidate; no destination has been touched.
    pub(crate) fn apply(self, mut candidate: Candidate) -> Result<Candidate, ItemBridgeError> {
        for edit in self.0 {
            let result = match edit.operation {
                Operation::Set(value) => candidate.stack.set_raw(edit.key, value),
                Operation::Remove => candidate.stack.remove_raw(edit.key),
                Operation::Reset => candidate.stack.reset_raw(&edit.key),
            };
            result.map_err(|_| ItemBridgeError::InvalidEdit("component registry changed"))?;
        }
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item_bridge::SnapshotStore;
    use foton_registry::data_components::vanilla_components::{CUSTOM_NAME, DAMAGE, GLIDER};
    use foton_registry::{init_vanilla_registry, item_stack::ItemStack, vanilla_items};
    use std::num::NonZeroUsize;
    use text_components::TextComponent;

    #[test]
    fn unrelated_set_preserves_removal_and_reset_reveals_prototype() {
        init_vanilla_registry();
        let store = SnapshotStore::new(NonZeroUsize::MIN);
        let mut original = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
        original.set(GLIDER, ());
        original.remove(DAMAGE);
        let key = store.capture(&original).expect("capture").publish();
        let edits = Edits::new(vec![Edit {
            key: CUSTOM_NAME.key().clone(),
            operation: Operation::Set(ComponentData::new(TextComponent::plain("edited"))),
        }])
        .expect("edits");
        let candidate = edits
            .apply(store.materialize(key).expect("candidate"))
            .expect("apply");
        assert!(candidate.stack.has(GLIDER));
        assert!(!candidate.stack.has(DAMAGE));
        let candidate = Edits::new(vec![Edit {
            key: DAMAGE.key().clone(),
            operation: Operation::Reset,
        }])
        .expect("reset journal")
        .apply(candidate)
        .expect("reset");
        assert_eq!(candidate.stack.get(DAMAGE), Some(&0));
        let candidate = Edits::new(vec![Edit {
            key: DAMAGE.key().clone(),
            operation: Operation::Remove,
        }])
        .expect("remove journal")
        .apply(candidate)
        .expect("remove");
        assert!(!candidate.stack.has(DAMAGE));
        assert!(
            !store
                .lookup(key)
                .expect("original")
                .stack()
                .has(CUSTOM_NAME)
        );
    }

    #[test]
    fn duplicate_and_invalid_middle_edits_are_rejected_as_a_whole() {
        init_vanilla_registry();
        let remove = || Edit {
            key: DAMAGE.key().clone(),
            operation: Operation::Remove,
        };
        assert!(Edits::new(vec![remove(), remove()]).is_err());
        assert!(
            Edits::new(vec![
                remove(),
                Edit {
                    key: GLIDER.key().clone(),
                    operation: Operation::Set(ComponentData::new(12_i32))
                }
            ])
            .is_err()
        );
    }
}
