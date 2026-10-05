use super::*;
use foton_protocol::packets::game::TeamMethod;
use jni::objects::{JObject, JValue};

pub(crate) fn check(env: &mut JNIEnv<'_>, world: &JString<'_>, scoreboard: &Scoreboard) {
    let board = env
        .new_object(
            "foton/FotonScoreboard",
            "(Ljava/lang/String;)V",
            &[JValue::Object(world)],
        )
        .expect("public scoreboard handle");
    let name = env.new_string("review-team").expect("team name");
    let team = env
        .call_method(
            &board,
            "registerNewTeam",
            "(Ljava/lang/String;)Lorg/bukkit/scoreboard/Team;",
            &[JValue::Object(&name)],
        )
        .expect("register team")
        .l()
        .expect("team");
    let native = scoreboard.team("review-team").expect("native team");
    assert_eq!(scoreboard.take_team_updates().len(), 1);
    let display = TeamDisplay {
        color: TeamColor::DarkPurple,
        name_tag_visibility: TeamVisibility::HideForOtherTeams,
        collision_rule: TeamCollisionRule::Never,
        ..TeamDisplay::default()
    };
    assert!(scoreboard.set_team_display(&native, display));
    assert_eq!(scoreboard.take_team_updates().len(), 1);
    for property in ["prefix", "displayName", "suffix"] {
        for json in [
            r#""plain""#,
            r#"{"text":"styled","bold":true,"color":"gold","extra":[" child"]}"#,
        ] {
            let before = scoreboard.team_display(&native);
            let json_arg = env.new_string(json).expect("JSON");
            let value = env
                .call_static_method(
                    "foton/ComponentJson",
                    "parse",
                    "(Ljava/lang/String;)Lnet/kyori/adventure/text/Component;",
                    &[JValue::Object(&json_arg)],
                )
                .expect("Adventure component")
                .l()
                .expect("component");
            let result = env.call_method(
                &team,
                property,
                "(Lnet/kyori/adventure/text/Component;)V",
                &[JValue::Object(&value)],
            );
            if result.is_err() {
                env.exception_describe().expect("setter diagnostic");
            }
            result.expect("actual public setter JNI");
            let expected = adventure_component_from_json(json).expect("expected component");
            let mut display = before;
            match property {
                "prefix" => display.prefix = Some(expected.clone()),
                "suffix" => display.suffix = Some(expected.clone()),
                _ => display.display_name = Some(expected.clone()),
            }
            assert_eq!(
                scoreboard.team_display(&native),
                display,
                "other presentation fields unchanged"
            );
            let updates = scoreboard.take_team_updates();
            assert_eq!(updates.len(), 1, "one queued mutation per setter");
            assert!(matches!(updates[0].method, TeamMethod::Change(_)));
            let read = env
                .call_method(
                    &team,
                    property,
                    "()Lnet/kyori/adventure/text/Component;",
                    &[],
                )
                .expect("public getter")
                .l()
                .expect("component");
            let encoded = env
                .call_static_method(
                    "foton/ComponentJson",
                    "json",
                    "(Lnet/kyori/adventure/text/Component;)Ljava/lang/String;",
                    &[JValue::Object(&read)],
                )
                .expect("getter JSON")
                .l()
                .expect("JSON");
            let encoded: String = env
                .get_string(&JString::from(encoded))
                .expect("string")
                .into();
            assert_eq!(adventure_component_from_json(&encoded), Some(expected));
        }
    }
    check_clear(env, &team, scoreboard);
}

fn check_clear(env: &mut JNIEnv<'_>, team: &JObject<'_>, scoreboard: &Scoreboard) {
    let native = scoreboard.team("review-team").expect("native team");
    let empty = env
        .call_static_method(
            "net/kyori/adventure/text/Component",
            "empty",
            "()Lnet/kyori/adventure/text/TextComponent;",
            &[],
        )
        .expect("empty component")
        .l()
        .expect("component");
    env.call_method(
        team,
        "prefix",
        "(Lnet/kyori/adventure/text/Component;)V",
        &[JValue::Object(&empty)],
    )
    .expect("clear prefix");
    assert!(scoreboard.team_display(&native).prefix.is_none());
    assert_eq!(scoreboard.take_team_updates().len(), 1);
    assert!(
        env.call_method(
            team,
            "prefix",
            "(Lnet/kyori/adventure/text/Component;)V",
            &[JValue::Object(&JObject::null())]
        )
        .is_err()
    );
    env.exception_clear().expect("expected null rejection");
    assert!(scoreboard.take_team_updates().is_empty());
    for property in ["displayName", "suffix"] {
        env.call_method(
            team,
            property,
            "(Lnet/kyori/adventure/text/Component;)V",
            &[JValue::Object(&JObject::null())],
        )
        .expect("null restores default");
        assert_eq!(scoreboard.take_team_updates().len(), 1);
    }
    let display = scoreboard.team_display(&native);
    assert!(display.display_name.is_none() && display.suffix.is_none());
    assert_eq!(display.color, TeamColor::DarkPurple);
}
