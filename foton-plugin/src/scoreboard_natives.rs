//! `org.bukkit.scoreboard.Team`, answered by the scoreboard `/team` uses.
//!
//! Every native names a world and reaches that world's domain scoreboard, the
//! one selectors, friendly fire and `/team` already read. A change queues the
//! team packet vanilla would send; the server sends it to the domain's
//! players on its next tick, which is what makes a prefix appear in game.

use std::ptr::null_mut;

use foton_core::scoreboard::{ScoreHolder, Scoreboard, TeamDisplay};
use foton_protocol::packets::game::{TeamCollisionRule, TeamColor, TeamVisibility};
use foton_utils::Identifier;
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jobjectArray, jstring};
use serde::Serialize;
use serde::de::DeserializeOwned;
use text_components::TextComponent;

use crate::natives::{server, string_array, to_java};

fn text(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Option<String> {
    env.get_string(value).ok().map(Into::into)
}

/// Runs `answer` against the scoreboard of `world`'s domain.
fn with_scoreboard<T>(world: &str, answer: impl FnOnce(&Scoreboard) -> T) -> Option<T> {
    let server = server()?;
    let key: Identifier = world.parse().ok()?;
    let world = server.worlds.get_owned(&key)?;
    server.scoreboards.get(world.domain()).map(answer)
}

/// `foton.Native.scoreboardTeamNames`
pub(crate) extern "system" fn team_names(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world: JString<'_>,
) -> jobjectArray {
    let names = text(&mut env, &world)
        .and_then(|world| with_scoreboard(&world, Scoreboard::team_names))
        .unwrap_or_default();
    string_array(&mut env, &names)
}

/// `foton.Native.scoreboardRegisterTeam`: false when the name is taken or empty.
pub(crate) extern "system" fn register_team(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world: JString<'_>,
    team: JString<'_>,
) -> jboolean {
    let (Some(world), Some(team)) = (text(&mut env, &world), text(&mut env, &team)) else {
        return 0;
    };
    jboolean::from(
        with_scoreboard(&world, |scoreboard| scoreboard.add_team(team).is_ok()).unwrap_or(false),
    )
}

/// `foton.Native.scoreboardUnregisterTeam`
pub(crate) extern "system" fn unregister_team(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world: JString<'_>,
    team: JString<'_>,
) -> jboolean {
    let (Some(world), Some(team)) = (text(&mut env, &world), text(&mut env, &team)) else {
        return 0;
    };
    let removed = with_scoreboard(&world, |scoreboard| {
        scoreboard
            .team(&team)
            .is_some_and(|team| scoreboard.remove_team(&team))
    });
    jboolean::from(removed.unwrap_or(false))
}

/// `foton.Native.scoreboardAddTeamEntry`: moves the entry onto the team.
pub(crate) extern "system" fn add_team_entry(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world: JString<'_>,
    team: JString<'_>,
    entry: JString<'_>,
) -> jboolean {
    let (Some(world), Some(team), Some(entry)) = (
        text(&mut env, &world),
        text(&mut env, &team),
        text(&mut env, &entry),
    ) else {
        return 0;
    };
    let added = with_scoreboard(&world, |scoreboard| {
        scoreboard.team(&team).is_some_and(|team| {
            scoreboard
                .add_holder_to_team(&ScoreHolder::new(entry), &team)
                .is_ok()
        })
    });
    jboolean::from(added.unwrap_or(false))
}

/// `foton.Native.scoreboardRemoveTeamEntry`: only an entry on this very team leaves it.
pub(crate) extern "system" fn remove_team_entry(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world: JString<'_>,
    team: JString<'_>,
    entry: JString<'_>,
) -> jboolean {
    let (Some(world), Some(team), Some(entry)) = (
        text(&mut env, &world),
        text(&mut env, &team),
        text(&mut env, &entry),
    ) else {
        return 0;
    };
    let removed = with_scoreboard(&world, |scoreboard| {
        let holder = ScoreHolder::new(entry);
        scoreboard.holder_team_name(&holder).as_deref() == Some(team.as_str())
            && scoreboard.remove_holder_from_team(&holder)
    });
    jboolean::from(removed.unwrap_or(false))
}

fn component_json(component: Option<&TextComponent>) -> Option<String> {
    component.map_or_else(
        || Some(String::new()),
        |component| serde_json::to_string(component).ok(),
    )
}

fn serialized<T: Serialize>(value: &T) -> Option<String> {
    match serde_json::to_value(value).ok()? {
        serde_json::Value::String(name) => Some(name),
        _ => None,
    }
}

fn deserialized<T: DeserializeOwned>(name: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(name.to_owned())).ok()
}

/// `foton.Native.scoreboardTeamProperty`: a team's property as text, or null
/// when the team does not exist.
///
/// Components are JSON text, an empty prefix or suffix is the empty string,
/// enums are their snake-case names and flags are `true` or `false`.
pub(crate) extern "system" fn team_property(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world: JString<'_>,
    team: JString<'_>,
    property: JString<'_>,
) -> jstring {
    let (Some(world), Some(team), Some(property)) = (
        text(&mut env, &world),
        text(&mut env, &team),
        text(&mut env, &property),
    ) else {
        return null_mut();
    };
    let value = with_scoreboard(&world, |scoreboard| {
        let team = scoreboard.team(&team)?;
        let display = scoreboard.team_display(&team);
        let options = scoreboard.team_options(&team);
        match property.as_str() {
            "displayName" => display.display_name.as_ref().map_or_else(
                || serde_json::to_string(&TextComponent::plain(team.name().to_owned())).ok(),
                |name| serde_json::to_string(name).ok(),
            ),
            "prefix" => component_json(display.prefix.as_ref()),
            "suffix" => component_json(display.suffix.as_ref()),
            "color" => serialized(&display.color),
            "nameTagVisibility" => serialized(&display.name_tag_visibility),
            "collisionRule" => serialized(&display.collision_rule),
            "friendlyFire" => Some(options.allow_friendly_fire.to_string()),
            "seeFriendlyInvisibles" => Some(options.see_friendly_invisibles.to_string()),
            _ => None,
        }
    })
    .flatten();
    to_java(&mut env, value)
}

/// A component property's new value: empty clears it, anything else is JSON text.
fn parse_component(value: &str) -> Result<Option<TextComponent>, serde_json::Error> {
    if value.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(value).map(Some)
}

fn apply_display(display: &mut TeamDisplay, property: &str, value: &str) -> Option<()> {
    match property {
        "displayName" => display.display_name = parse_component(value).ok()?,
        "prefix" => display.prefix = parse_component(value).ok()?,
        "suffix" => display.suffix = parse_component(value).ok()?,
        "color" => display.color = deserialized::<TeamColor>(value)?,
        "nameTagVisibility" => display.name_tag_visibility = deserialized::<TeamVisibility>(value)?,
        "collisionRule" => display.collision_rule = deserialized::<TeamCollisionRule>(value)?,
        _ => return None,
    }
    Some(())
}

/// `foton.Native.scoreboardSetTeamProperty`: false for a missing team or an
/// unreadable value; the forms are those `scoreboardTeamProperty` answers in.
pub(crate) extern "system" fn set_team_property(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world: JString<'_>,
    team: JString<'_>,
    property: JString<'_>,
    value: JString<'_>,
) -> jboolean {
    let (Some(world), Some(team), Some(property), Some(value)) = (
        text(&mut env, &world),
        text(&mut env, &team),
        text(&mut env, &property),
        text(&mut env, &value),
    ) else {
        return 0;
    };
    let changed = with_scoreboard(&world, |scoreboard| {
        let team = scoreboard.team(&team)?;
        match property.as_str() {
            "friendlyFire" | "seeFriendlyInvisibles" => {
                let flag: bool = value.parse().ok()?;
                let mut options = scoreboard.team_options(&team);
                if property == "friendlyFire" {
                    options.allow_friendly_fire = flag;
                } else {
                    options.see_friendly_invisibles = flag;
                }
                scoreboard.set_team_options(&team, options).then_some(())
            }
            _ => {
                let mut display = scoreboard.team_display(&team);
                apply_display(&mut display, &property, &value)?;
                scoreboard.set_team_display(&team, display).then_some(())
            }
        }
    })
    .flatten();
    jboolean::from(changed.is_some())
}

#[cfg(test)]
mod tests {
    use foton_core::scoreboard::TeamDisplay;
    use foton_protocol::packets::game::TeamColor;

    use super::{apply_display, deserialized, serialized};

    /// Java names colours by their snake-case form, and so does the scoreboard's save.
    #[test]
    fn colours_cross_by_name() {
        assert_eq!(
            serialized(&TeamColor::DarkPurple).as_deref(),
            Some("dark_purple")
        );
        assert_eq!(deserialized::<TeamColor>("reset"), Some(TeamColor::Reset));
        assert_eq!(deserialized::<TeamColor>("purple"), None);
    }

    /// A prefix is JSON text; the empty string takes it away again.
    #[test]
    fn a_prefix_is_set_and_cleared() {
        let mut display = TeamDisplay::default();
        assert!(
            apply_display(
                &mut display,
                "prefix",
                r#"{"text":"[Hylien] ","color":"gold"}"#
            )
            .is_some()
        );
        assert!(display.prefix.is_some());
        assert!(apply_display(&mut display, "prefix", "").is_some());
        assert!(display.prefix.is_none());
        assert!(apply_display(&mut display, "prefix", "{not json").is_none());
    }
}
