//! Display entities and the interaction entity: `org.bukkit.entity.Display`
//! and its block, item and text kinds, and `Interaction`.
//!
//! Everything here is synced entity data. The client interpolates a display
//! between the values it is sent, so setting the interpolation duration and
//! delay, or the teleport duration, is the whole of the animation: one packet
//! of metadata and the client does the rest.

use std::ffi::c_void;

use foton_core::entity::SharedEntity;
use foton_core::entity::entities::objects::display_ui::{
    BlockDisplayEntity, FLAG_SEE_THROUGH, FLAG_SHADOW, FLAG_USE_DEFAULT_BACKGROUND,
    INITIAL_BACKGROUND, InteractionEntity, ItemDisplayContext, ItemDisplayEntity, TextAlign,
    TextDisplayEntity,
};
use foton_registry::entity_data::{Quaternionf, Vector3f};
use foton_registry::vanilla_entity_data::DisplayEntityData;
use foton_utils::Downcast as _;
use foton_utils::text::DisplayResolutor;
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jbyte, jdoubleArray, jfloat, jint, jstring};

use super::support::{component, doubles, entity, method, text};
use super::{describe_slot, describe_state, parse_slot, to_java};

/// Vanilla's `Display.POS_ROT_INTERPOLATION_DURATION` ceiling.
const MAX_TELEPORT_DURATION: i32 = 59;

/// Runs `f` on the shared `Display` data of a block, item or text display.
fn with_display<R>(
    entity: &SharedEntity,
    f: impl FnOnce(&mut DisplayEntityData) -> R,
) -> Option<R> {
    if let Some(display) = entity.as_ref().downcast_ref::<BlockDisplayEntity>() {
        return Some(f(&mut display.entity_data().lock().display));
    }
    if let Some(display) = entity.as_ref().downcast_ref::<ItemDisplayEntity>() {
        return Some(f(&mut display.entity_data().lock().display));
    }
    if let Some(display) = entity.as_ref().downcast_ref::<TextDisplayEntity>() {
        return Some(f(&mut display.entity_data().lock().display));
    }
    None
}

fn display(env: &mut JNIEnv<'_>, uuid: &JString<'_>, f: impl FnOnce(&mut DisplayEntityData)) {
    if let Some((_, entity)) = entity(env, uuid) {
        with_display(&entity, f);
    }
}

/// Every `Display` field in one read, so the values belong to one moment:
/// interpolation duration, interpolation delay, teleport duration, billboard,
/// packed brightness, view range, shadow radius, shadow strength, width,
/// height, glow colour, then translation, scale, left and right rotation.
extern "system" fn display_state(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let state = entity(&mut env, &uuid).and_then(|(_, entity)| {
        with_display(&entity, |data| {
            let translation = *data.translation.get();
            let scale = *data.scale.get();
            let left = *data.left_rotation.get();
            let right = *data.right_rotation.get();
            vec![
                f64::from(*data.transformation_interpolation_duration.get()),
                f64::from(*data.transformation_interpolation_start_delta_ticks.get()),
                f64::from(*data.pos_rot_interpolation_duration.get()),
                f64::from(*data.billboard_render_constraints.get()),
                f64::from(*data.brightness_override.get()),
                f64::from(*data.view_range.get()),
                f64::from(*data.shadow_radius.get()),
                f64::from(*data.shadow_strength.get()),
                f64::from(*data.width.get()),
                f64::from(*data.height.get()),
                f64::from(*data.glow_color_override.get()),
                f64::from(translation.x),
                f64::from(translation.y),
                f64::from(translation.z),
                f64::from(scale.x),
                f64::from(scale.y),
                f64::from(scale.z),
                f64::from(left.x),
                f64::from(left.y),
                f64::from(left.z),
                f64::from(left.w),
                f64::from(right.x),
                f64::from(right.y),
                f64::from(right.z),
                f64::from(right.w),
            ]
        })
    });
    doubles(&mut env, state.as_deref())
}

extern "system" fn set_display_interpolation_duration(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ticks: jint,
) {
    display(&mut env, &uuid, |data| {
        data.transformation_interpolation_duration.set(ticks);
    });
}

/// Sent even when unchanged: the client restarts the interpolation each time
/// the delay arrives, which is how a plugin replays the same animation.
extern "system" fn set_display_interpolation_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ticks: jint,
) {
    display(&mut env, &uuid, |data| {
        data.transformation_interpolation_start_delta_ticks
            .set_forced(ticks);
    });
}

extern "system" fn set_display_teleport_duration(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ticks: jint,
) {
    display(&mut env, &uuid, |data| {
        data.pos_rot_interpolation_duration
            .set(ticks.clamp(0, MAX_TELEPORT_DURATION));
    });
}

/// Vanilla's billboard ids are Bukkit's `Billboard` ordinals.
extern "system" fn set_display_billboard(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    billboard: jint,
) {
    let Ok(billboard) = i8::try_from(billboard) else {
        return;
    };
    display(&mut env, &uuid, |data| {
        data.billboard_render_constraints.set(billboard)
    });
}

/// Vanilla's `Brightness.pack`; -1 for either light clears the override.
extern "system" fn set_display_brightness(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    block: jint,
    sky: jint,
) {
    let packed = if block < 0 || sky < 0 {
        -1
    } else {
        block.clamp(0, 15) << 4 | sky.clamp(0, 15) << 20
    };
    display(&mut env, &uuid, |data| data.brightness_override.set(packed));
}

/// The ARGB colour a glowing display outlines in; -1 keeps the team colour.
extern "system" fn set_display_glow_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    argb: jint,
) {
    display(&mut env, &uuid, |data| data.glow_color_override.set(argb));
}

/// One of the float fields, by the index `display_state` reads it at.
extern "system" fn set_display_float(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    field: jint,
    value: jfloat,
) {
    if !value.is_finite() {
        return;
    }
    display(&mut env, &uuid, |data| match field {
        5 => data.view_range.set(value),
        6 => data.shadow_radius.set(value),
        7 => data.shadow_strength.set(value),
        8 => data.width.set(value),
        9 => data.height.set(value),
        _ => {}
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "a transformation is fourteen floats"
)]
extern "system" fn set_display_transformation(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    tx: jfloat,
    ty: jfloat,
    tz: jfloat,
    sx: jfloat,
    sy: jfloat,
    sz: jfloat,
    lx: jfloat,
    ly: jfloat,
    lz: jfloat,
    lw: jfloat,
    rx: jfloat,
    ry: jfloat,
    rz: jfloat,
    rw: jfloat,
) {
    display(&mut env, &uuid, |data| {
        data.translation.set(Vector3f::new(tx, ty, tz));
        data.scale.set(Vector3f::new(sx, sy, sz));
        data.left_rotation.set(Quaternionf::new(lx, ly, lz, lw));
        data.right_rotation.set(Quaternionf::new(rx, ry, rz, rw));
    });
}

/// The block state a block display renders, as block data text.
extern "system" fn block_display_block(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let state = entity(&mut env, &uuid).and_then(|(_, entity)| {
        let display = entity.as_ref().downcast_ref::<BlockDisplayEntity>()?;
        describe_state(*display.entity_data().lock().block_state.get())
    });
    to_java(&mut env, state)
}

extern "system" fn item_display_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let item = entity(&mut env, &uuid).and_then(|(_, entity)| {
        let display = entity.as_ref().downcast_ref::<ItemDisplayEntity>()?;
        Some(describe_slot(&display.item_stack()))
    });
    to_java(&mut env, item)
}

extern "system" fn set_item_display_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    item: JString<'_>,
) {
    let Some(stack) = text(&mut env, &item).and_then(|value| parse_slot(&value)) else {
        return;
    };
    if let Some((_, entity)) = entity(&mut env, &uuid)
        && let Some(display) = entity.as_ref().downcast_ref::<ItemDisplayEntity>()
    {
        display.set_item_stack(stack);
    }
}

/// The item model context, by vanilla's serialized name (`head`, `gui`, ...).
extern "system" fn item_display_transform(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let name = entity(&mut env, &uuid).and_then(|(_, entity)| {
        let display = entity.as_ref().downcast_ref::<ItemDisplayEntity>()?;
        Some(display.item_transform().serialized_name().to_owned())
    });
    to_java(&mut env, name)
}

extern "system" fn set_item_display_transform(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
) {
    let Some(context) =
        text(&mut env, &name).and_then(|value| ItemDisplayContext::from_serialized_name(&value))
    else {
        return;
    };
    if let Some((_, entity)) = entity(&mut env, &uuid)
        && let Some(display) = entity.as_ref().downcast_ref::<ItemDisplayEntity>()
    {
        display.set_item_transform(context);
    }
}

fn with_text_display(env: &mut JNIEnv<'_>, uuid: &JString<'_>, f: impl FnOnce(&TextDisplayEntity)) {
    if let Some((_, entity)) = entity(env, uuid)
        && let Some(display) = entity.as_ref().downcast_ref::<TextDisplayEntity>()
    {
        f(display);
    }
}

/// The text, from a component's JSON.
extern "system" fn set_text_display_text(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    json: JString<'_>,
) {
    let Some(text) = component(&mut env, &json) else {
        return;
    };
    with_text_display(&mut env, &uuid, |display| display.set_text(text));
}

/// The text's plain content; its formatting is what players see.
extern "system" fn text_display_plain_text(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let plain = entity(&mut env, &uuid).and_then(|(_, entity)| {
        let display = entity.as_ref().downcast_ref::<TextDisplayEntity>()?;
        Some(display.text().to_plain(&DisplayResolutor))
    });
    to_java(&mut env, plain)
}

/// `{line width, background ARGB, text opacity, style flags}`.
extern "system" fn text_display_state(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let state = entity(&mut env, &uuid).and_then(|(_, entity)| {
        let display = entity.as_ref().downcast_ref::<TextDisplayEntity>()?;
        Some([
            f64::from(display.line_width()),
            f64::from(display.background_color()),
            f64::from(display.text_opacity()),
            f64::from(display.style_flags()),
        ])
    });
    doubles(&mut env, state.as_ref().map(<[f64; 4]>::as_slice))
}

extern "system" fn set_text_display_line_width(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    width: jint,
) {
    with_text_display(&mut env, &uuid, |display| display.set_line_width(width));
}

extern "system" fn set_text_display_background(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    argb: jint,
) {
    with_text_display(&mut env, &uuid, |display| {
        display.set_background_color(argb)
    });
}

extern "system" fn set_text_display_opacity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    opacity: jbyte,
) {
    with_text_display(&mut env, &uuid, |display| display.set_text_opacity(opacity));
}

/// Back to vanilla's default background, what Paper does for a null colour.
extern "system" fn reset_text_display_background(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) {
    with_text_display(&mut env, &uuid, |display| {
        display.set_background_color(INITIAL_BACKGROUND)
    });
}

/// One boolean style flag: `shadow`, `see_through` or `default_background`.
extern "system" fn set_text_display_flag(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    flag: JString<'_>,
    enabled: jboolean,
) {
    let flag = match text(&mut env, &flag).as_deref() {
        Some("shadow") => FLAG_SHADOW,
        Some("see_through") => FLAG_SEE_THROUGH,
        Some("default_background") => FLAG_USE_DEFAULT_BACKGROUND,
        _ => return,
    };
    with_text_display(&mut env, &uuid, |display| {
        display.set_style_flag(flag, enabled != 0)
    });
}

/// The alignment by vanilla's name: `center`, `left` or `right`.
extern "system" fn set_text_display_alignment(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
) {
    let Some(align) =
        text(&mut env, &name).and_then(|value| TextAlign::from_serialized_name(&value))
    else {
        return;
    };
    with_text_display(&mut env, &uuid, |display| display.set_align(align));
}

/// `{width, height, responsive}` of an interaction entity.
extern "system" fn interaction_state(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let state = entity(&mut env, &uuid).and_then(|(_, entity)| {
        let interaction = entity.as_ref().downcast_ref::<InteractionEntity>()?;
        Some([
            f64::from(interaction.width()),
            f64::from(interaction.height()),
            f64::from(u8::from(interaction.response())),
        ])
    });
    doubles(&mut env, state.as_ref().map(<[f64; 3]>::as_slice))
}

/// Width (0), height (1) or responsiveness (2, non-zero for true).
extern "system" fn set_interaction_value(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    field: jint,
    value: jfloat,
) {
    if !value.is_finite() {
        return;
    }
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return;
    };
    let Some(interaction) = entity.as_ref().downcast_ref::<InteractionEntity>() else {
        return;
    };
    match field {
        0 => interaction.set_width(value),
        1 => interaction.set_height(value),
        2 => interaction.set_response(value != 0.0),
        _ => {}
    }
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "displayState",
            "(Ljava/lang/String;)[D",
            display_state as *mut c_void,
        ),
        method(
            "setDisplayInterpolationDuration",
            "(Ljava/lang/String;I)V",
            set_display_interpolation_duration as *mut c_void,
        ),
        method(
            "setDisplayInterpolationDelay",
            "(Ljava/lang/String;I)V",
            set_display_interpolation_delay as *mut c_void,
        ),
        method(
            "setDisplayTeleportDuration",
            "(Ljava/lang/String;I)V",
            set_display_teleport_duration as *mut c_void,
        ),
        method(
            "setDisplayBillboard",
            "(Ljava/lang/String;I)V",
            set_display_billboard as *mut c_void,
        ),
        method(
            "setDisplayBrightness",
            "(Ljava/lang/String;II)V",
            set_display_brightness as *mut c_void,
        ),
        method(
            "setDisplayGlowColor",
            "(Ljava/lang/String;I)V",
            set_display_glow_color as *mut c_void,
        ),
        method(
            "setDisplayFloat",
            "(Ljava/lang/String;IF)V",
            set_display_float as *mut c_void,
        ),
        method(
            "setDisplayTransformation",
            "(Ljava/lang/String;FFFFFFFFFFFFFF)V",
            set_display_transformation as *mut c_void,
        ),
        method(
            "blockDisplayBlock",
            "(Ljava/lang/String;)Ljava/lang/String;",
            block_display_block as *mut c_void,
        ),
        method(
            "itemDisplayItem",
            "(Ljava/lang/String;)Ljava/lang/String;",
            item_display_item as *mut c_void,
        ),
        method(
            "setItemDisplayItem",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_item_display_item as *mut c_void,
        ),
        method(
            "itemDisplayTransform",
            "(Ljava/lang/String;)Ljava/lang/String;",
            item_display_transform as *mut c_void,
        ),
        method(
            "setItemDisplayTransform",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_item_display_transform as *mut c_void,
        ),
        method(
            "setTextDisplayText",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_text_display_text as *mut c_void,
        ),
        method(
            "textDisplayPlainText",
            "(Ljava/lang/String;)Ljava/lang/String;",
            text_display_plain_text as *mut c_void,
        ),
        method(
            "textDisplayState",
            "(Ljava/lang/String;)[D",
            text_display_state as *mut c_void,
        ),
        method(
            "setTextDisplayLineWidth",
            "(Ljava/lang/String;I)V",
            set_text_display_line_width as *mut c_void,
        ),
        method(
            "setTextDisplayBackground",
            "(Ljava/lang/String;I)V",
            set_text_display_background as *mut c_void,
        ),
        method(
            "resetTextDisplayBackground",
            "(Ljava/lang/String;)V",
            reset_text_display_background as *mut c_void,
        ),
        method(
            "setTextDisplayOpacity",
            "(Ljava/lang/String;B)V",
            set_text_display_opacity as *mut c_void,
        ),
        method(
            "setTextDisplayFlag",
            "(Ljava/lang/String;Ljava/lang/String;Z)V",
            set_text_display_flag as *mut c_void,
        ),
        method(
            "setTextDisplayAlignment",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_text_display_alignment as *mut c_void,
        ),
        method(
            "interactionState",
            "(Ljava/lang/String;)[D",
            interaction_state as *mut c_void,
        ),
        method(
            "setInteractionValue",
            "(Ljava/lang/String;IF)V",
            set_interaction_value as *mut c_void,
        ),
    ]
}
