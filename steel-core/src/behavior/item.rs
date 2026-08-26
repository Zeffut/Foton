//! Item behavior trait and registry.

use std::sync::Arc;

use std::borrow::Cow;
use steel_registry::data_components::vanilla_components::{
    BLOCKS_ATTACKS, CONSUMABLE, FOOD, KINETIC_WEAPON, POTION_CONTENTS, USE_REMAINDER,
};

use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::data_components::vanilla_components::ITEM_NAME;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::equipment::EquipmentSlot;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::sound_events;
use steel_registry::{REGISTRY, RegistryEntry, RegistryExt};
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId};
use text_components::TextComponent;

use crate::behavior::items::DefaultItemBehavior;
use crate::behavior::{InteractionResult, UseItemContext, UseOnContext};
use crate::block_entity::entities::{SignBlockEntity, SignText};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ItemEntity;
use crate::entity::{Entity, LivingEntity, MobEffectInstance};
use crate::inventory::click::MouseButton;
use crate::inventory::lock::ContainerLockGuard;
use crate::inventory::slots::slot::Slot;
use crate::player::{Player, player_inventory::EquipmentSwapResult};
use crate::world::World;
use steel_registry::data_components::PotionContents;
use steel_registry::mob_effect::MobEffectRef;

pub use steel_registry::data_components::vanilla_components::ItemUseAnimation;

/// Applies the vanilla consume effects to `stack` and returns the resulting stack.
///
/// Vanilla parity: `Consumable.onConsume`. Shared by the instant path in
/// `ItemBehavior::use_item` and by `ItemBehavior::finish_using`, so both apply
/// exactly the same effects.
fn apply_consume_effects(
    stack: &mut ItemStack,
    world: &Arc<World>,
    user: &dyn LivingEntity,
) -> ItemStack {
    let Some(consumable) = stack.get(CONSUMABLE).cloned() else {
        return stack.copy_with_count(stack.count());
    };
    let position = user.position();

    // Vanilla `emitParticlesAndSounds`: NEUTRAL, volume 1.0, pitch triangle(1.0, 0.4).
    if let Some(sound) = consumable.sound().registry_ref() {
        let pitch = 0.4f32.mul_add(rand::random::<f32>() - rand::random::<f32>(), 1.0);
        world.play_sound_at(sound, SoundSource::Neutral, position, 1.0, pitch, None);
    }
    // TODO: emit the consume particles once item-driven particles are supported.

    // Vanilla dispatches to every `ConsumableListener` carried by the stack.
    // `FoodProperties` is the only one Steel models today.
    if let Some(player) = user.as_player()
        && let Some(food) = stack.get(FOOD)
    {
        player
            .food_data
            .lock()
            .eat_food(food.nutrition(), food.saturation());
        let burp_pitch = 0.1f32.mul_add(rand::random::<f32>(), 0.9);
        world.play_sound_at(
            &sound_events::ENTITY_PLAYER_BURP,
            SoundSource::Players,
            position,
            0.5,
            burp_pitch,
            None,
        );
    }

    // Vanilla parity: `PotionContents.onConsume`, which is the listener that
    // makes a brewed potion do anything at all. Without it a player can brew,
    // bottle and drink and get only the sound.
    if let Some(contents) = stack.get(POTION_CONTENTS) {
        for (effect, duration, amplifier) in potion_effects(contents) {
            user.add_mob_effect(MobEffectInstance::with_duration(
                effect, duration, amplifier,
            ));
        }
    }

    // TODO: apply the remaining `consumable.on_consume_effects()` kinds --
    // teleport, remove-effects, play-sound -- once they are modeled.
    // TODO: emit the EAT / DRINK game event once game-event dispatch exists.

    // Vanilla `ItemStack.consume` leaves creative-mode stacks untouched.
    if user.has_infinite_materials() {
        return stack.copy_with_count(stack.count());
    }

    let consumed = stack.copy_with_count(stack.count().saturating_sub(1));
    // Vanilla parity: `ItemStack.applyAfterUseComponentSideEffects` hands back
    // what the container becomes -- the glass bottle from a potion, the bowl
    // from a stew. Only when the stack is spent; a second bottle in the stack
    // is still a potion.
    if consumed.is_empty()
        && let Some(remainder) = stack.get(USE_REMAINDER)
    {
        return remainder.convert_into().create();
    }
    consumed
}

/// Yields every effect a potion bottle carries.
///
/// Vanilla parity: `PotionContents.forEachEffect`, without the duration scale,
/// which only differs from one outside the ominous-bottle path.
#[must_use]
pub fn potion_effects(contents: &PotionContents) -> Vec<(MobEffectRef, i32, i32)> {
    let mut effects = Vec::new();

    if let Some(potion) = contents.potion() {
        for effect in potion.value().effects {
            effects.push((effect.effect, effect.duration, effect.amplifier));
        }
    }
    for effect in contents.custom_effects() {
        effects.push((effect.effect(), effect.duration(), effect.amplifier()));
    }

    effects
}

/// Returns vanilla `BlockState.getDestroySpeed(level, pos)`.
///
/// Steel keeps hardness on the block's config, so the level and position vanilla
/// threads through are unnecessary.
pub(crate) fn block_destroy_time(state: BlockStateId) -> f32 {
    REGISTRY
        .blocks
        .by_state_id(state)
        .map_or(0.0, |block| block.config.destroy_time)
}

/// An item that changes a sign's text when it is clicked on one.
///
/// Vanilla parity: the `SignApplicator` interface. The ink sacs, the dyes and
/// honeycomb implement it and carry no `useOn` for signs of their own: the sign
/// block drives them from `SignBlock.useItemOn`, which is also what enforces the
/// waxed check and pays for the change out of the stack.
pub trait SignApplicator {
    /// Applies this item to one side of a sign, returning whether it changed.
    ///
    /// Vanilla parity: `SignApplicator.tryApplyToSign`. A no-op -- a second glow
    /// ink sac on already glowing text -- is a refusal, and the caller then
    /// neither consumes the item nor counts the interaction. Playing the sound
    /// belongs to the implementation, as it does in vanilla.
    fn try_apply_to_sign(
        &self,
        world: &Arc<World>,
        sign: &SignBlockEntity,
        is_front_text: bool,
        stack: &ItemStack,
        player: &Player,
    ) -> bool;

    /// Returns whether this item may be applied to the given side at all.
    ///
    /// Vanilla parity: `SignApplicator.canApplyToSign`, whose default refuses a
    /// blank side -- there is nothing to recolor or make glow.
    fn can_apply_to_sign(&self, text: &SignText, _stack: &ItemStack, _player: &Player) -> bool {
        text.has_message()
    }
}

/// Trait defining the behavior of an item.
///
/// This trait handles dynamic/functional aspects of items:
/// - Use on blocks (placing, interacting)
/// - Use in air
/// - etc.
pub trait ItemBehavior: Send + Sync {
    /// Returns the Rust type name of the concrete behavior implementation.
    #[cfg(feature = "flint")]
    #[must_use]
    #[expect(clippy::absolute_paths, reason = "easier for features")]
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Returns vanilla `Item.getName(stack)`.
    fn get_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        stack
            .get(ITEM_NAME)
            .map_or_else(|| Cow::Owned(TextComponent::new()), Cow::Borrowed)
    }

    /// Called when this item is used on a block.
    fn use_on(&self, _context: &mut UseOnContext) -> InteractionResult {
        InteractionResult::Pass
    }

    /// Called by vanilla `Item.mineBlock` once this item has broken `state`.
    ///
    /// Returns whether the stack acted as a tool, mirroring vanilla's return.
    /// The default is `Item.mineBlock` itself: a tool spends `damage_per_block`
    /// durability unless the block breaks instantly. Items whose durability rule
    /// differs -- shears, which pay for zero-hardness plants but never for fire --
    /// override this.
    ///
    /// Steel deviation: vanilla also takes the level and position so it can call
    /// `state.getDestroySpeed(level, pos)`. Steel reads hardness off the block
    /// state's config, which no vanilla block varies by position.
    fn mine_block(
        &self,
        stack: &mut ItemStack,
        state: BlockStateId,
        miner: &dyn LivingEntity,
    ) -> bool {
        let Some(damage_per_block) = stack.get_tool().map(|tool| tool.damage_per_block) else {
            return false;
        };

        if block_destroy_time(state) != 0.0 && damage_per_block > 0 {
            stack.hurt_and_break(damage_per_block, miner.has_infinite_materials());
        }

        true
    }

    /// Returns the arrow entity this item becomes when a weapon fires it.
    ///
    /// Vanilla parity: the entity built by `ArrowItem.createArrow`. `None` means
    /// the item is not an `ArrowItem`, which is what makes
    /// `ProjectileWeaponItem.createProjectile` fall back to `Items.ARROW`.
    fn arrow_entity_type(&self) -> Option<EntityTypeRef> {
        None
    }

    /// Returns whether holding this item lets `user` break `state`.
    ///
    /// Vanilla parity: `Item.canDestroyBlock`. The default refuses only when a
    /// tool opts out of creative-mode block breaking; the debug stick refuses
    /// always and uses the call as its left-click hook.
    fn can_destroy_block(
        &self,
        stack: &mut ItemStack,
        _state: BlockStateId,
        _world: &Arc<World>,
        _pos: BlockPos,
        user: &dyn LivingEntity,
    ) -> bool {
        stack.can_destroy_blocks_in_creative() || !user.has_infinite_materials()
    }

    /// Returns whether this item may be put inside a bundle or a shulker box.
    ///
    /// Vanilla parity: `Item.canFitInsideContainerItems`.
    fn can_fit_inside_container_items(&self) -> bool {
        true
    }

    /// Handles a click that carries this item onto `slot`, returning whether it
    /// replaced the menu's normal pickup handling.
    ///
    /// Vanilla parity: `Item.overrideStackedOnOther`, reached from
    /// `AbstractContainerMenu.tryItemClickBehaviourOverride` with the carried
    /// stack as `self`.
    fn override_stacked_on_other(
        &self,
        _stack: &mut ItemStack,
        _slot: &dyn Slot,
        _guard: &mut ContainerLockGuard,
        _button: MouseButton,
        _player: &Player,
    ) -> bool {
        false
    }

    /// Handles a click that carries `carried` onto this item, returning whether
    /// it replaced the menu's normal pickup handling.
    ///
    /// Vanilla parity: `Item.overrideOtherStackedOnMe`, where `stack` is the
    /// clicked slot's live item -- Vanilla mutates it in place, and so does
    /// this, including on the paths that return `false`.
    ///
    /// Steel deviation, twice over. Vanilla passes both the carried stack and a
    /// `SlotAccess` onto it, but its one call site supplies the menu's carried
    /// stack for both, so Steel takes it once. And Vanilla passes the whole
    /// `Slot`, of which the implementations only ask `allowModification`; Steel
    /// passes that answer instead so the live stack can be borrowed at the same
    /// time.
    fn override_other_stacked_on_me(
        &self,
        _stack: &mut ItemStack,
        _carried: &mut ItemStack,
        _allow_modification: bool,
        _button: MouseButton,
        _player: &Player,
    ) -> bool {
        false
    }

    /// Called when the item entity carrying this stack is destroyed.
    ///
    /// Vanilla parity: `Item.onDestroyed`, which container items use to spill
    /// what they hold instead of taking it with them.
    fn on_destroyed(&self, _entity: &ItemEntity) {}

    /// Returns this behavior as a sign applicator, when it is one.
    ///
    /// Vanilla parity: the `itemStack.getItem() instanceof SignApplicator` test
    /// in `SignBlock.useItemOn`. Steel has no class hierarchy to ask, so a
    /// behavior that applies to signs says so here.
    fn as_sign_applicator(&self) -> Option<&dyn SignApplicator> {
        None
    }

    /// Returns the block this item places, when it places one.
    ///
    /// Vanilla parity: the `itemStack.getItem() instanceof BlockItem blockItem`
    /// test callers follow with `blockItem.getBlock()` -- `Bee.mobInteract` is
    /// the one that matters here, because it is how a flower held out to a bee
    /// becomes the effect the flower carries.
    fn placed_block(&self) -> Option<BlockRef> {
        None
    }

    /// Called when this item is used (e.g. right click in air).
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        // Vanilla parity: `Consumable.startConsuming`.
        let consumable = context.inv.with_item(|item| item.get(CONSUMABLE).cloned());
        if let Some(consumable) = consumable {
            // Vanilla parity: `Consumable.canConsume`. Only food gates on hunger.
            let can_consume = context.inv.with_item(|item| {
                item.get(FOOD)
                    .is_none_or(|food| context.player.can_eat(food.can_always_eat()))
            });
            if !can_consume {
                return InteractionResult::Fail;
            }

            if consumable.consume_ticks() > 0 {
                context.player.start_using_item(context.hand);
                return InteractionResult::Consume;
            }

            // Consumables with no duration resolve on the spot.
            let world = context.world;
            let player = context.player;
            context.inv.with_item(|item| {
                *item = apply_consume_effects(item, world, player);
            });
            return InteractionResult::Consume;
        }

        // Vanilla parity: `Equippable.swapWithEquipmentSlot`. An item carrying
        // the component but not swappable falls straight through -- which is
        // exactly how a shield, whose `equippable` only names the off hand,
        // reaches the blocking branch below instead of being swapped.
        let equippable = context.inv.with_item(|item| item.get_equippable().cloned());
        if let Some(equippable) = equippable
            && equippable.swappable
        {
            if !equippable.can_be_equipped_by(context.player.entity_type()) {
                return InteractionResult::Pass;
            }

            let slot = equippable.slot;
            let result = context.inv.with_inventory(|inventory| {
                inventory.try_swap_with_equipment_slot(
                    context.hand,
                    slot,
                    context.player.has_infinite_materials(),
                )
            });

            return match result {
                EquipmentSwapResult::Success(overflow) => {
                    if !overflow.is_empty() {
                        let _ = context.player.drop_item(overflow, false, false);
                    }
                    InteractionResult::Success
                }
                EquipmentSwapResult::Fail => InteractionResult::Fail,
            };
        }

        // Vanilla parity: the `BLOCKS_ATTACKS` branch of `Item.use`. Raising a
        // shield is nothing more than starting to use it.
        if context.inv.with_item(|item| item.has(BLOCKS_ATTACKS)) {
            context.player.start_using_item(context.hand);
            return InteractionResult::Consume;
        }

        // TODO: mirror the `KINETIC_WEAPON` branch of `Item.use` once spears
        // and the wind-up sound they make are modeled.
        InteractionResult::Pass
    }

    /// Returns vanilla `Item.getUseAnimation`.
    fn get_use_animation(&self, stack: &ItemStack) -> ItemUseAnimation {
        if let Some(consumable) = stack.get(CONSUMABLE) {
            consumable.animation()
        } else if stack.has(BLOCKS_ATTACKS) {
            ItemUseAnimation::Block
        } else if stack.has(KINETIC_WEAPON) {
            ItemUseAnimation::Spear
        } else {
            ItemUseAnimation::None
        }
    }

    /// How far a mob will stand off and still fire this weapon.
    ///
    /// Vanilla parity: `ProjectileWeaponItem.getDefaultProjectileRange`, which
    /// only the bow and the crossbow answer. `None` means "not a projectile
    /// weapon", which is what puts a mob holding it back in melee range.
    fn default_projectile_range(&self) -> Option<i32> {
        None
    }

    /// Returns vanilla `Item.getUseDuration`.
    fn get_use_duration(&self, stack: &ItemStack, _user: &dyn LivingEntity) -> i32 {
        if let Some(consumable) = stack.get(CONSUMABLE) {
            consumable.consume_ticks()
        } else if stack.has(BLOCKS_ATTACKS) || stack.has(KINETIC_WEAPON) {
            72000
        } else {
            0
        }
    }

    /// Called once per tick for every stack a living entity carries.
    ///
    /// Vanilla parity: `Item.inventoryTick`, driven by `Inventory.tick` for the
    /// main inventory and by `EntityEquipment.tick` for the worn slots. `slot`
    /// is `None` for the inventory slots vanilla does not name -- everything
    /// except the selected hand.
    fn inventory_tick(
        &self,
        _stack: &mut ItemStack,
        _world: &Arc<World>,
        _owner: &dyn LivingEntity,
        _slot: Option<EquipmentSlot>,
    ) {
    }

    /// Called every tick while a living entity is actively using this item.
    fn on_use_tick(
        &self,
        _world: &Arc<World>,
        _user: &dyn LivingEntity,
        _stack: &mut ItemStack,
        _ticks_remaining: i32,
    ) {
    }

    /// Called when active use is released before completion.
    ///
    /// Returns whether vanilla should update active use once more before stopping it.
    fn release_using(
        &self,
        _stack: &mut ItemStack,
        _world: &Arc<World>,
        _user: &dyn LivingEntity,
        _time_left: i32,
    ) -> bool {
        false
    }

    /// Called when active use reaches its full duration.
    fn finish_using(
        &self,
        stack: &mut ItemStack,
        world: &Arc<World>,
        user: &dyn LivingEntity,
    ) -> ItemStack {
        if stack.has(CONSUMABLE) {
            return apply_consume_effects(stack, world, user);
        }
        stack.copy_with_count(stack.count())
    }

    /// Called by vanilla `ItemStack.interactLivingEntity`.
    fn interact_living_entity(
        &self,
        _stack: &mut ItemStack,
        _player: &Player,
        _target: &dyn LivingEntity,
        _hand: InteractionHand,
    ) -> InteractionResult {
        InteractionResult::Pass
    }

    /// Returns vanilla `Item.getItemDamageSource`.
    fn get_item_damage_source(&self, _attacker: &dyn LivingEntity) -> Option<DamageSource> {
        None
    }

    /// Returns item-specific attack damage added by `Item.getAttackDamageBonus`.
    fn get_attack_damage_bonus(
        &self,
        _attacker: &dyn LivingEntity,
        _victim: &dyn Entity,
        _base_damage: f32,
        _damage_source: &DamageSource,
    ) -> f32 {
        0.0
    }

    /// Called by vanilla `Item.hurtEnemy`.
    fn hurt_enemy(
        &self,
        _stack: &mut ItemStack,
        _target: &dyn LivingEntity,
        _attacker: &dyn LivingEntity,
    ) {
    }

    /// Called by vanilla `Item.postHurtEnemy`.
    fn post_hurt_enemy(
        &self,
        _stack: &mut ItemStack,
        _target: &dyn LivingEntity,
        _attacker: &dyn LivingEntity,
    ) {
    }

    /// Returns how much durability this weapon consumes after a successful entity hit.
    fn item_damage_per_attack(&self, stack: &ItemStack) -> Option<i32> {
        stack
            .get_weapon()
            .map(|weapon| weapon.item_damage_per_attack)
    }
}

/// Registry for item behaviors.
///
/// Created after the main registry is frozen. Block items get `BlockItemBehavior`,
/// other items get `DefaultItemBehavior`. Custom behaviors can be registered.
pub struct ItemBehaviorRegistry {
    behaviors: Vec<Box<dyn ItemBehavior>>,
}

impl ItemBehaviorRegistry {
    /// Creates a new behavior registry with default behaviors for all items.
    ///
    /// Call `register_item_behaviors()` after this to set up proper behaviors.
    #[must_use]
    pub fn new() -> Self {
        let item_count = REGISTRY.items.len();
        let behaviors = (0..item_count)
            .map(|_| Box::new(DefaultItemBehavior) as Box<dyn ItemBehavior>)
            .collect();

        Self { behaviors }
    }

    /// Sets a custom behavior for an item.
    pub fn set_behavior(&mut self, item: ItemRef, behavior: Box<dyn ItemBehavior>) {
        let id = item.id();
        self.behaviors[id] = behavior;
    }

    /// Gets the behavior for an item.
    #[must_use]
    pub fn get_behavior(&self, item: ItemRef) -> &dyn ItemBehavior {
        let id = item.id();
        self.behaviors[id].as_ref()
    }

    /// Returns vanilla `ItemStack.getHoverName`, including item-specific
    /// `Item.getName(stack)` overrides when no custom name is present.
    #[must_use]
    pub fn hover_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        stack
            .custom_name()
            .unwrap_or_else(|| self.get_behavior(stack.item()).get_name(stack))
    }

    /// Get all behaviors.
    #[cfg(feature = "flint")]
    #[must_use]
    pub fn get_behaviors(&self) -> &[Box<dyn ItemBehavior>] {
        &self.behaviors
    }
}

impl Default for ItemBehaviorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod potion_tests {
    use steel_registry::data_components::PotionContents;
    use steel_registry::data_components::vanilla_components::POTION_CONTENTS;
    use steel_registry::mob_effect::instance::MobEffectInstance as RegistryMobEffectInstance;
    use steel_registry::registry::reference::RegistryReference;
    use steel_registry::{
        init_vanilla_registry, item_stack::ItemStack, vanilla_items, vanilla_mob_effects,
        vanilla_potions,
    };

    use super::potion_effects;
    use steel_registry::potion::Potion;

    fn bottle_of(potion: &'static Potion) -> ItemStack {
        let mut stack = ItemStack::new(&vanilla_items::POTION);
        stack.set(
            POTION_CONTENTS,
            PotionContents::new(Some(RegistryReference::new(potion)), None, Vec::new(), None),
        );
        stack
    }

    #[test]
    fn a_water_bottle_grants_nothing() {
        init_vanilla_registry();
        let water = bottle_of(&vanilla_potions::WATER);
        let contents = water.get(POTION_CONTENTS).expect("bottle holds contents");
        assert!(potion_effects(contents).is_empty());
    }

    #[test]
    fn a_swiftness_bottle_grants_speed_for_its_own_duration() {
        init_vanilla_registry();
        let swiftness = bottle_of(&vanilla_potions::SWIFTNESS);
        let contents = swiftness.get(POTION_CONTENTS).expect("holds contents");
        let effects = potion_effects(contents);

        assert_eq!(effects.len(), 1);
        let (effect, duration, amplifier) = effects[0];
        assert_eq!(effect, vanilla_mob_effects::SPEED);
        assert!(duration > 0, "a timed potion must carry a duration");
        assert_eq!(amplifier, 0);
    }

    #[test]
    fn the_long_variant_lasts_longer_than_the_plain_one() {
        init_vanilla_registry();
        let plain = bottle_of(&vanilla_potions::SWIFTNESS);
        let long = bottle_of(&vanilla_potions::LONG_SWIFTNESS);

        let plain_duration = potion_effects(plain.get(POTION_CONTENTS).expect("contents"))[0].1;
        let long_duration = potion_effects(long.get(POTION_CONTENTS).expect("contents"))[0].1;
        assert!(long_duration > plain_duration);
    }

    #[test]
    fn the_strong_variant_raises_the_amplifier_rather_than_the_duration() {
        init_vanilla_registry();
        let plain = bottle_of(&vanilla_potions::SWIFTNESS);
        let strong = bottle_of(&vanilla_potions::STRONG_SWIFTNESS);

        let plain_amplifier = potion_effects(plain.get(POTION_CONTENTS).expect("contents"))[0].2;
        let strong_amplifier = potion_effects(strong.get(POTION_CONTENTS).expect("contents"))[0].2;
        assert!(strong_amplifier > plain_amplifier);
    }

    #[test]
    fn custom_effects_come_through_alongside_the_potion() {
        init_vanilla_registry();
        let mut stack = ItemStack::new(&vanilla_items::POTION);
        stack.set(
            POTION_CONTENTS,
            PotionContents::new(
                Some(RegistryReference::new(&vanilla_potions::SWIFTNESS)),
                None,
                vec![RegistryMobEffectInstance::simple(
                    vanilla_mob_effects::JUMP_BOOST,
                    200,
                    1,
                )],
                None,
            ),
        );

        let effects = potion_effects(stack.get(POTION_CONTENTS).expect("contents"));
        assert_eq!(
            effects.len(),
            2,
            "the potion's own effect and the custom one"
        );
        assert!(
            effects
                .iter()
                .any(|(effect, _, _)| *effect == vanilla_mob_effects::JUMP_BOOST)
        );
    }
}
