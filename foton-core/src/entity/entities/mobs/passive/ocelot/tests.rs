use std::io::Cursor;
use std::sync::Weak;

use foton_registry::init_vanilla_registry;
use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;

use super::*;

fn ocelot() -> OcelotEntity {
    init_vanilla_registry();
    OcelotEntity::new(&vanilla_entities::OCELOT, 1, DVec3::ZERO, Weak::new())
}

/// Vanilla parity: `Ocelot.removeWhenFarAway`. Trust is what makes an ocelot
/// permanent -- there is no taming to do it instead.
#[test]
fn only_an_untrusting_ocelot_despawns_when_it_gets_old() {
    let ocelot = ocelot();

    for _ in 0..=UNTRUSTING_DESPAWN_AGE_TICKS {
        ocelot.advance_tick_count();
    }
    assert!(Mob::remove_when_far_away(&ocelot, 4096.0));

    ocelot.set_trusting(true);
    assert!(!Mob::remove_when_far_away(&ocelot, 4096.0));
}

/// Trust is the only ocelot state worth persisting, and losing it on reload
/// would turn a settled ocelot back into a despawning one.
#[test]
fn trust_survives_a_save() {
    let ocelot = ocelot();
    ocelot.set_trusting(true);

    let mut nbt = NbtCompound::new();
    ocelot.save_additional(&mut nbt);
    assert_eq!(nbt.byte("Trusting"), Some(1));

    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("ocelot save data should reborrow: {error}"));

    let reloaded = OcelotEntity::new(&vanilla_entities::OCELOT, 2, DVec3::ZERO, Weak::new());
    reloaded.load_additional((&borrowed).into());

    assert!(reloaded.is_trusting());
}

/// An ocelot is not a tameable animal, and code that treats every pet as one
/// would give it an owner it can never have.
#[test]
fn an_ocelot_is_not_a_tamable_animal() {
    let ocelot = ocelot();

    assert!(ocelot.as_tamable_animal().is_none());
}
