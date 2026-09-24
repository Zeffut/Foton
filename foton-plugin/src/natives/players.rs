//! Player state a plugin reads and writes through `org.bukkit.entity.Player`
//! and `HumanEntity`, and the living-entity flags players are the usual
//! subject of (gliding, swimming).

use std::ffi::c_void;

use foton_core::entity::Entity as _;
use foton_protocol::packets::game::{CClearTitles, CSystemChat, CTabList};
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint, jstring};
use text_components::TextComponent;

use super::support::{component, entity, key, method, player, text};
use super::{describe_slot, parse_slot, player_tab_lists, to_java};

/// The stack the entity is holding up (a drawn bow, a raised shield, food
/// being eaten), or the empty stack.
extern "system" fn active_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let item = entity(&mut env, &uuid).and_then(|(_, entity)| {
        let living = entity.as_living_entity()?;
        Some(
            living
                .use_item()
                .map(|stack| describe_slot(&stack))
                .unwrap_or_default(),
        )
    });
    to_java(&mut env, item)
}

const CLIMBING: jint = 1;
const IN_LAVA: jint = 2;
const IN_WATER: jint = 4;
const RIPTIDING: jint = 8;
const IN_RAIN: jint = 16;
/// Vanilla's `LivingEntity.LIVING_ENTITY_FLAG_SPIN_ATTACK`.
const SPIN_ATTACK_FLAG: i8 = 4;

/// Where the entity is, as bits read at one moment: on a climbable block, in
/// lava, in water (a bubble column is water), riptiding, in rain.
///
/// Riptiding is vanilla's `isAutoSpinAttack`: the spin-attack bit of the
/// synced living flags, which is what every client renders the spin from.
extern "system" fn entity_surroundings(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return 0;
    };
    let mut bits = 0;
    if entity.is_in_water() {
        bits |= IN_WATER;
    }
    if entity.is_in_lava() {
        bits |= IN_LAVA;
    }
    if entity.is_in_rain() {
        bits |= IN_RAIN;
    }
    if entity.on_climbable() {
        bits |= CLIMBING;
    }
    if let Some(living) = entity.as_living_entity()
        && living
            .living_synced_data()
            .is_some_and(|data| data.living_entity_flags() & SPIN_ATTACK_FLAG != 0)
    {
        bits |= RIPTIDING;
    }
    bits
}

/// Clears the title and subtitle on screen, keeping the fade times: Paper's
/// `clearTitle`, where `resetTitle` also resets the times.
extern "system" fn clear_player_title(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) {
    if let Some(player) = player(&mut env, &uuid) {
        player.send_packet(CClearTitles { reset_times: false });
    }
}

/// Ticks left on a cooldown group -- an item id or a `use_cooldown` group.
extern "system" fn player_cooldown(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    group: JString<'_>,
) -> jint {
    let Some(group) = key(&mut env, &group) else {
        return 0;
    };
    player(&mut env, &uuid).map_or(0, |player| player.item_cooldown_remaining(&group))
}

extern "system" fn set_player_cooldown(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    group: JString<'_>,
    ticks: jint,
) {
    let Some(group) = key(&mut env, &group) else {
        return;
    };
    if let Some(player) = player(&mut env, &uuid) {
        player.add_item_cooldown_group(group, ticks.max(0));
    }
}

/// A cooldown on the group this stack belongs to: its `use_cooldown` group,
/// or its item.
extern "system" fn set_player_item_cooldown(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    item: JString<'_>,
    ticks: jint,
) {
    let Some(stack) = text(&mut env, &item).and_then(|value| parse_slot(&value)) else {
        return;
    };
    if stack.is_empty() {
        return;
    }
    if let Some(player) = player(&mut env, &uuid) {
        player.add_item_cooldown(&stack, ticks.max(0));
    }
}

/// The last input the client reported, as vanilla's `Input` bit flags.
extern "system" fn player_input(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    player(&mut env, &uuid).map_or(0, |player| jint::from(player.last_client_input().flags()))
}

extern "system" fn player_cursor(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let cursor = player(&mut env, &uuid).map(|player| describe_slot(&player.carried_item()));
    to_java(&mut env, cursor)
}

extern "system" fn set_player_cursor(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    item: JString<'_>,
) {
    let Some(stack) = text(&mut env, &item).and_then(|value| parse_slot(&value)) else {
        return;
    };
    if let Some(player) = player(&mut env, &uuid) {
        let _ = player.set_carried_item(stack);
    }
}

/// Experience as an orb would give it: with `mending`, damaged mending gear
/// soaks it up first and only the rest reaches the bar.
extern "system" fn give_player_experience(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    amount: jint,
    mending: jboolean,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let remaining = if mending != 0 && amount > 0 {
        player
            .inventory
            .lock()
            .repair_random_equipped_item_with_xp(amount)
    } else {
        amount
    };
    if remaining != 0 {
        player.give_experience_points(remaining);
    }
}

/// An action bar message from a component's JSON, formatting kept.
extern "system" fn send_action_bar_component(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    json: JString<'_>,
) {
    let Some(message) = component(&mut env, &json) else {
        return;
    };
    if let Some(player) = player(&mut env, &uuid) {
        player.send_packet(CSystemChat::new(&message, true, player.as_ref()));
    }
}

/// The player's name in the tab list; null restores the plain name.
extern "system" fn set_player_list_name_component(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    json: JString<'_>,
) {
    let name = component(&mut env, &json);
    if name.is_none() && !json.is_null() {
        return;
    }
    if let Some(player) = player(&mut env, &uuid) {
        player.set_tab_list_name(name);
    }
}

/// Both halves of the tab list decoration at once, from components' JSON; a
/// null half is empty, as Paper's `sendPlayerListHeaderAndFooter` has it.
extern "system" fn set_player_list_header_footer_components(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    header: JString<'_>,
    footer: JString<'_>,
) {
    let header = component(&mut env, &header).unwrap_or_else(|| TextComponent::plain(""));
    let footer = component(&mut env, &footer).unwrap_or_else(|| TextComponent::plain(""));
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    player_tab_lists()
        .write()
        .insert(player.uuid(), (header.clone(), footer.clone()));
    player.send_packet(CTabList::new(&header, &footer, player.as_ref()));
}

/// Vanilla's fall-flying shared flag. A player's client reports its own
/// gliding each tick, so a plugin's change lasts as long as the client agrees.
extern "system" fn set_entity_gliding(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    gliding: jboolean,
) {
    if let Some((_, entity)) = entity(&mut env, &uuid) {
        entity.set_shared_fall_flying(gliding != 0);
    }
}

/// Vanilla's `setSwimming`, the swimming shared flag.
extern "system" fn set_entity_swimming(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    swimming: jboolean,
) {
    if let Some((_, entity)) = entity(&mut env, &uuid) {
        entity.set_shared_swimming(swimming != 0);
    }
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "activeItem",
            "(Ljava/lang/String;)Ljava/lang/String;",
            active_item as *mut c_void,
        ),
        method(
            "entitySurroundings",
            "(Ljava/lang/String;)I",
            entity_surroundings as *mut c_void,
        ),
        method(
            "clearPlayerTitle",
            "(Ljava/lang/String;)V",
            clear_player_title as *mut c_void,
        ),
        method(
            "playerCooldown",
            "(Ljava/lang/String;Ljava/lang/String;)I",
            player_cooldown as *mut c_void,
        ),
        method(
            "setPlayerCooldown",
            "(Ljava/lang/String;Ljava/lang/String;I)V",
            set_player_cooldown as *mut c_void,
        ),
        method(
            "setPlayerItemCooldown",
            "(Ljava/lang/String;Ljava/lang/String;I)V",
            set_player_item_cooldown as *mut c_void,
        ),
        method(
            "playerInput",
            "(Ljava/lang/String;)I",
            player_input as *mut c_void,
        ),
        method(
            "playerCursor",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_cursor as *mut c_void,
        ),
        method(
            "setPlayerCursor",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_player_cursor as *mut c_void,
        ),
        method(
            "givePlayerExperience",
            "(Ljava/lang/String;IZ)V",
            give_player_experience as *mut c_void,
        ),
        method(
            "sendActionBarComponent",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            send_action_bar_component as *mut c_void,
        ),
        method(
            "setPlayerListNameComponent",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_player_list_name_component as *mut c_void,
        ),
        method(
            "setPlayerListHeaderFooterComponents",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            set_player_list_header_footer_components as *mut c_void,
        ),
        method(
            "setEntityGliding",
            "(Ljava/lang/String;Z)V",
            set_entity_gliding as *mut c_void,
        ),
        method(
            "setEntitySwimming",
            "(Ljava/lang/String;Z)V",
            set_entity_swimming as *mut c_void,
        ),
    ]
}
