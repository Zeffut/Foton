//! Global registry and behavior initialization.

use std::time::Instant;

use foton_registry::{
    init_vanilla_registry_with, registry::plugin::register_queued_plugin_enchantments,
};

use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::init_entities;

fn fill_behavior_registries() {
    init_behaviors();
    init_block_entities();
    init_entities();
    log::info!("Behavior registries initialized");
}

/// # Errors
/// Returns an error if the global registry has already been initialized.
pub(crate) fn init_globals() -> Result<(), String> {
    let start = Instant::now();
    // Plugin bootstrap declarations are queued by the JVM before this phase.
    // They must be appended while the registry is still mutable; publication
    // freezes every registry and makes late registration invalid.
    let published = init_vanilla_registry_with(|registry| {
        if let Err(error) = register_queued_plugin_enchantments(&mut registry.enchantments) {
            log::error!("plugin enchantment bootstrap rejected: {error:?}");
        }
    });
    log::info!("Vanilla registry loaded in {:?}", start.elapsed());

    if !published {
        return Err("global registry has already been initialized".to_owned());
    }

    fill_behavior_registries();
    Ok(())
}

/// Idempotent [`init_globals`] for tests and benchmarks, which bootstrap
/// repeatedly in one process.
#[cfg(any(test, feature = "benchmark-support"))]
pub fn init_globals_once() {
    use std::sync::Once;

    static INIT: Once = Once::new();
    INIT.call_once(|| {
        init_vanilla_registry_with(|registry| {
            if let Err(error) = register_queued_plugin_enchantments(&mut registry.enchantments) {
                log::error!("plugin enchantment bootstrap rejected: {error:?}");
            }
        });
        fill_behavior_registries();
    });
}
