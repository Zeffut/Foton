//! `/scoreboard` against a live test server: the command writes the same
//! per-domain scoreboard that `execute if score` and `execute store` use.

use super::*;
use crate::scoreboard::ScoreHolder;
use foton_protocol::packets::game::{NumberFormat, ObjectiveRenderType};
use text_components::{TextComponent, format::Color};

fn run(server: &Arc<Server>, command: &str) -> i32 {
    server.run_command_now(server.function_source(), command)
}

fn score(server: &Arc<Server>, domain: &str, holder: &str, objective: &str) -> Option<i32> {
    let scoreboard = server.scoreboards.get(domain)?;
    scoreboard.score(&ScoreHolder::new(holder), &scoreboard.objective(objective)?)
}

#[test]
fn scoreboard_commands_share_the_state_execute_reads_and_writes() {
    foton_registry::init_vanilla_registry();
    let world = fresh_test_world("scoreboard_command_shared_state");
    let domain = world.domain().to_owned();
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let storage = test_storage_root("scoreboard-command-shared-state");
        let server = test_server(Arc::clone(&world), PermissionSubjectIndex::new(), &storage)
            .await
            .expect("server");
        server.attach_worlds();

        // The datapack line from the bug report parses with no objective yet,
        // and fails at run time instead of at load.
        let line = "scoreboard players set craft_toggle_ craft_toggle_ 1";
        assert_eq!(run(&server, line), 0);
        assert_eq!(
            run(&server, "scoreboard objectives add craft_toggle_ dummy"),
            1
        );
        assert_eq!(run(&server, line), 1);
        assert_eq!(
            score(&server, &domain, "craft_toggle_", "craft_toggle_"),
            Some(1)
        );

        // Arithmetic, then a condition reading the result.
        assert_eq!(run(&server, "scoreboard objectives add n dummy \"N\""), 1);
        assert_eq!(run(&server, "scoreboard players set a n 7"), 1);
        assert_eq!(run(&server, "scoreboard players add a n 5"), 1);
        assert_eq!(run(&server, "scoreboard players remove a n 2"), 1);
        assert_eq!(
            run(
                &server,
                "execute if score a n matches 10 run scoreboard players set hit n 1"
            ),
            1
        );
        assert_eq!(score(&server, &domain, "hit", "n"), Some(1));

        // `players get` is an `execute store` source, and `store` writes back.
        assert_eq!(
            run(
                &server,
                "execute store result score b n run scoreboard players get a n"
            ),
            1
        );
        assert_eq!(score(&server, &domain, "b", "n"), Some(10));
        assert_eq!(run(&server, "scoreboard players operation a n *= b n"), 1);
        assert_eq!(score(&server, &domain, "a", "n"), Some(100));
        assert_eq!(run(&server, "scoreboard players operation a n >< b n"), 1);
        assert_eq!(
            (
                score(&server, &domain, "a", "n"),
                score(&server, &domain, "b", "n")
            ),
            (Some(10), Some(100))
        );

        // A zero divisor stops the operation; a missing score cannot be read.
        assert_eq!(run(&server, "scoreboard players set zero n 0"), 1);
        assert_eq!(
            run(&server, "scoreboard players operation a n /= zero n"),
            0
        );
        assert_eq!(score(&server, &domain, "a", "n"), Some(10));
        assert_eq!(run(&server, "scoreboard players get nobody n"), 0);

        // `*` is every tracked holder; reset takes scores away.
        assert_eq!(run(&server, "scoreboard players set * n 3"), 1);
        assert_eq!(score(&server, &domain, "b", "n"), Some(3));
        assert_eq!(run(&server, "scoreboard players reset * n"), 1);
        assert_eq!(score(&server, &domain, "b", "n"), None);
        assert_eq!(run(&server, "scoreboard players list"), 1);

        // Removing the objective takes its scores; reads then fail cleanly.
        assert_eq!(run(&server, "scoreboard objectives remove n"), 1);
        assert_eq!(run(&server, "scoreboard players get a n"), 0);
        assert_eq!(run(&server, "scoreboard objectives add n dummy"), 1);
        assert_eq!(score(&server, &domain, "a", "n"), None);

        drop(server);
        if let Err(error) = fs::remove_dir_all(&storage).await {
            panic!("test storage should be removed: {error}");
        }
    });
}

#[test]
fn display_names_and_number_formats_are_parsed_and_stored() {
    foton_registry::init_vanilla_registry();
    let world = fresh_test_world("scoreboard_command_formats");
    let domain = world.domain().to_owned();
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let storage = test_storage_root("scoreboard-command-formats");
        let server = test_server(Arc::clone(&world), PermissionSubjectIndex::new(), &storage)
            .await
            .expect("server");
        server.attach_worlds();
        let objective_data = |name: &str| {
            server
                .scoreboards
                .get(&domain)
                .and_then(|scoreboard| scoreboard.objective_data(name))
                .expect("objective")
        };

        assert_eq!(
            run(
                &server,
                r#"scoreboard objectives add n dummy {"text":"Coins","color":"gold"}"#
            ),
            1
        );
        assert_eq!(
            objective_data("n").display_name.format.color,
            Some(Color::Gold)
        );
        assert_eq!(
            run(&server, "scoreboard objectives modify n rendertype hearts"),
            1
        );
        assert_eq!(objective_data("n").render_type, ObjectiveRenderType::Hearts);
        assert_eq!(
            run(
                &server,
                "scoreboard objectives modify n displayautoupdate true"
            ),
            1
        );
        assert!(objective_data("n").display_auto_update);

        // Each number format form, then the bare node that clears it.
        assert_eq!(
            run(&server, "scoreboard objectives modify n numberformat blank"),
            1
        );
        assert_eq!(objective_data("n").number_format, Some(NumberFormat::Blank));
        assert_eq!(
            run(
                &server,
                r#"scoreboard objectives modify n numberformat fixed "x""#
            ),
            1
        );
        assert!(matches!(
            objective_data("n").number_format,
            Some(NumberFormat::Fixed(_))
        ));
        assert_eq!(
            run(
                &server,
                "scoreboard objectives modify n numberformat styled {bold:true,color:\"red\"}"
            ),
            1
        );
        let Some(NumberFormat::Styled(style)) = objective_data("n").number_format else {
            panic!("styled format should be stored");
        };
        assert_eq!(style.format.bold, Some(true));
        assert_eq!(style.format.color, Some(Color::Red));
        assert_eq!(
            run(&server, "scoreboard objectives modify n numberformat"),
            1
        );
        assert_eq!(objective_data("n").number_format, None);

        drop(server);
        if let Err(error) = fs::remove_dir_all(&storage).await {
            panic!("test storage should be removed: {error}");
        }
    });
}

#[test]
fn per_score_names_and_formats_create_the_score_they_decorate() {
    foton_registry::init_vanilla_registry();
    let world = fresh_test_world("scoreboard_command_score_display");
    let domain = world.domain().to_owned();
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let storage = test_storage_root("scoreboard-command-score-display");
        let server = test_server(Arc::clone(&world), PermissionSubjectIndex::new(), &storage)
            .await
            .expect("server");
        server.attach_worlds();
        assert_eq!(run(&server, "scoreboard objectives add n dummy"), 1);

        // Per-score name and format, which create the score they decorate.
        assert_eq!(
            run(
                &server,
                r#"scoreboard players display name Steve n "The Steve""#
            ),
            1
        );
        assert_eq!(
            run(
                &server,
                "scoreboard players display numberformat Steve n blank"
            ),
            1
        );
        let scoreboard = server.scoreboards.get(&domain).expect("scoreboard");
        let objective = scoreboard.objective("n").expect("handle");
        let entry = scoreboard
            .score_entry(&ScoreHolder::new("Steve"), &objective)
            .expect("score");
        assert_eq!(entry.display(), Some(&TextComponent::plain("The Steve")));
        assert_eq!(entry.number_format(), Some(&NumberFormat::Blank));
        assert_eq!(run(&server, "scoreboard players display name Steve n"), 1);

        // Malformed styles and criteria are refused at parse time.
        assert_eq!(
            run(
                &server,
                "scoreboard objectives modify n numberformat styled 5"
            ),
            0
        );
        assert_eq!(
            run(&server, "scoreboard objectives setdisplay nowhere n"),
            0
        );
        assert_eq!(
            run(&server, "scoreboard players operation Steve n ?? Steve n"),
            0
        );

        drop(server);
        if let Err(error) = fs::remove_dir_all(&storage).await {
            panic!("test storage should be removed: {error}");
        }
    });
}

#[test]
fn objective_criteria_decide_what_commands_may_do() {
    foton_registry::init_vanilla_registry();
    let world = fresh_test_world("scoreboard_command_criteria");
    let domain = world.domain().to_owned();
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let storage = test_storage_root("scoreboard-command-criteria");
        let server = test_server(Arc::clone(&world), PermissionSubjectIndex::new(), &storage)
            .await
            .expect("server");
        server.attach_worlds();

        // A read-only criterion refuses writes, an unknown one is refused outright.
        assert_eq!(run(&server, "scoreboard objectives add hp health"), 1);
        assert_eq!(run(&server, "scoreboard players set a hp 1"), 0);
        assert_eq!(run(&server, "scoreboard objectives add bad nonsense"), 0);
        assert_eq!(run(&server, "scoreboard objectives add hp dummy"), 0);
        assert_eq!(
            run(
                &server,
                "scoreboard objectives add jumps minecraft.custom:minecraft.jump"
            ),
            1
        );

        // `enable` only applies to triggers, and only counts what it unlocks.
        assert_eq!(run(&server, "scoreboard objectives add t trigger"), 1);
        assert_eq!(run(&server, "scoreboard players enable a hp"), 0);
        assert_eq!(run(&server, "scoreboard players enable a t"), 1);
        assert_eq!(run(&server, "scoreboard players enable a t"), 0);
        let scoreboard = server.scoreboards.get(&domain).expect("scoreboard");
        let trigger = scoreboard.objective("t").expect("trigger objective");
        let entry = scoreboard
            .score_entry(&ScoreHolder::new("a"), &trigger)
            .expect("enable creates the score");
        assert!(!entry.is_locked());

        // A slot refuses what it already shows; clearing an empty one fails.
        assert_eq!(
            run(&server, "scoreboard objectives setdisplay sidebar t"),
            1
        );
        assert_eq!(
            run(&server, "scoreboard objectives setdisplay sidebar t"),
            0
        );
        assert_eq!(run(&server, "scoreboard objectives setdisplay sidebar"), 1);
        assert_eq!(run(&server, "scoreboard objectives setdisplay sidebar"), 0);

        drop(server);
        if let Err(error) = fs::remove_dir_all(&storage).await {
            panic!("test storage should be removed: {error}");
        }
    });
}
