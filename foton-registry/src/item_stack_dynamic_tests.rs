use super::ItemStack;
use crate::data_components::{
    ComponentData,
    vanilla_components::{DAMAGE, GLIDER},
};
use crate::{init_vanilla_registry, vanilla_items};
use foton_utils::Identifier;

#[test]
fn checked_dynamic_edits_match_typed_edits_and_fail_atomically() {
    init_vanilla_registry();
    let mut dynamic = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    dynamic.set(GLIDER, ());
    let mut typed = dynamic.clone();
    dynamic
        .set_raw(DAMAGE.key().clone(), ComponentData::new(12_i32))
        .expect("set damage");
    typed.set(DAMAGE, 12);
    assert_eq!(dynamic, typed);
    dynamic
        .remove_raw(DAMAGE.key().clone())
        .expect("remove damage");
    typed.remove(DAMAGE);
    assert_eq!(dynamic, typed);
    dynamic.reset_raw(DAMAGE.key()).expect("reset damage");
    typed.clear(DAMAGE);
    assert_eq!(dynamic, typed);
    dynamic
        .set_raw(DAMAGE.key().clone(), ComponentData::new(0_i32))
        .expect("prototype set");
    typed.set(DAMAGE, 0);
    assert_eq!(dynamic.patch(), typed.patch());
    let before = dynamic.clone();
    let unknown = Identifier::vanilla_static("unknown_bridge_component");
    assert!(
        dynamic
            .set_raw(unknown.clone(), ComponentData::new(()))
            .is_err()
    );
    assert!(dynamic.remove_raw(unknown.clone()).is_err());
    assert!(dynamic.reset_raw(&unknown).is_err());
    assert!(
        dynamic
            .set_raw(DAMAGE.key().clone(), ComponentData::new(()))
            .is_err()
    );
    assert_eq!(dynamic, before);
    dynamic
        .remove_raw(GLIDER.key().clone())
        .expect("remove non-prototype");
    typed.remove(GLIDER);
    assert_eq!(dynamic.patch(), typed.patch());
}
