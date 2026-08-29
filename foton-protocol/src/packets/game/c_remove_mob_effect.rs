//! Clientbound remove mob effect packet.

use foton_macros::{ClientPacket, WriteTo};
use foton_registry::mob_effect::MobEffectRef;
use foton_registry::packets::play::C_REMOVE_MOB_EFFECT;

/// Sent when the client should remove an entity mob effect.
///
/// Vanilla: `ClientboundRemoveMobEffectPacket`.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_REMOVE_MOB_EFFECT)]
pub struct CRemoveMobEffect {
    #[write(as = VarInt)]
    pub entity_id: i32,
    /// Holder-registry mob effect id.
    #[write(as = VarInt)]
    pub effect_id: i32,
}

impl CRemoveMobEffect {
    #[must_use]
    pub fn new(entity_id: i32, effect: MobEffectRef) -> Self {
        Self {
            entity_id,
            effect_id: effect.packet_holder_id(),
        }
    }
}

#[cfg(test)]
mod tests {

    use foton_registry::init_vanilla_registry;
    use foton_registry::vanilla_mob_effects;

    use super::*;

    #[test]
    fn remove_mob_effect_uses_raw_holder_registry_id() {
        init_vanilla_registry();

        let packet = CRemoveMobEffect::new(42, vanilla_mob_effects::SPEED);

        assert_eq!(packet.effect_id, 0);
    }
}
