use super::*;
#[test]
fn sttr1_initialization_builds_consistent_mission_state() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 initialization example should be readable");
    let source = source.replace(
        "START_GAME",
        r#"INIT_MISSION
PRINT_LONG_SCAN
PRINT "MISSION ", START_STARDATE, " ", STARDATE, " ", MISSION_DURATION, " ", DEADLINE, " ", MAX_ENERGY, " ", ENERGY, " ", MAX_TORPEDOES, " ", TORPEDOES, " ", SHIELDS, " ", KLINGON_INIT_ENERGY, " ", ENT_QX, " ", ENT_QY, " ", ENT_SX, " ", ENT_SY
CR
PRINT "TOTALS ", KLINGONS_LEFT, " ", KLINGONS_INITIAL, " ", BASES_LEFT, " ", GENERATION_ATTEMPTS
CR
PRINT "QUADRANT ", ENT_QX, " ", ENT_QY, " ", KLINGONS_HERE, " ", BASES_HERE, " ", STARS_HERE
CR
PRINT "GALAXY "
LET QUADRANT_INDEX = 1
WHILE QUADRANT_INDEX <= 64
  PRINT @GALAXY[QUADRANT_INDEX], " "
  LET QUADRANT_INDEX = QUADRANT_INDEX + 1
ENDWH
CR
PRINT "CHART "
LET QUADRANT_INDEX = 1
WHILE QUADRANT_INDEX <= 64
  PRINT @CHART[QUADRANT_INDEX], " "
  LET QUADRANT_INDEX = QUADRANT_INDEX + 1
ENDWH
CR
PRINT "DAMAGE "
LET QUADRANT_INDEX = 1
WHILE QUADRANT_INDEX <= 8
  PRINT @DAMAGE[QUADRANT_INDEX], " "
  LET QUADRANT_INDEX = QUADRANT_INDEX + 1
ENDWH
CR
PRINT "COURSE_DX "
LET QUADRANT_INDEX = 1
WHILE QUADRANT_INDEX <= 9
  PRINT @COURSE_DX[QUADRANT_INDEX], " "
  LET QUADRANT_INDEX = QUADRANT_INDEX + 1
ENDWH
CR
PRINT "COURSE_DY "
LET QUADRANT_INDEX = 1
WHILE QUADRANT_INDEX <= 9
  PRINT @COURSE_DY[QUADRANT_INDEX], " "
  LET QUADRANT_INDEX = QUADRANT_INDEX + 1
ENDWH
CR
"#,
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        30,
    );

    let result = success(result);
    let mission = output_values(writer.text(), "MISSION ");
    assert_eq!(mission.len(), 14);
    assert!((2000..=3900).contains(&mission[0]));
    assert_eq!(mission[0] % 100, 0);
    assert_eq!(mission[1], mission[0]);
    assert_eq!(mission[2], 30);
    assert_eq!(mission[3], mission[0] + mission[2]);
    assert_eq!(&mission[4..10], &[3000, 3000, 10, 10, 0, 200]);
    assert!(mission[10..14].iter().all(|value| (1..=8).contains(value)));

    let totals = output_values(writer.text(), "TOTALS ");
    assert_eq!(totals.len(), 4);
    assert!(totals[0] > 0);
    assert_eq!(totals[1], totals[0]);
    assert!(totals[2] > 0);
    assert_eq!(totals[3], 2);

    let galaxy = output_values(writer.text(), "GALAXY ");
    assert_eq!(galaxy.len(), 64);
    let (mut klingons, mut bases) = (0, 0);
    for summary in galaxy {
        let quadrant_klingons = summary / 100;
        let quadrant_bases = summary / 10 % 10;
        let quadrant_stars = summary % 10;
        assert!((0..=3).contains(&quadrant_klingons));
        assert!((0..=1).contains(&quadrant_bases));
        assert!((1..=8).contains(&quadrant_stars));
        klingons += quadrant_klingons;
        bases += quadrant_bases;
    }
    assert_eq!(totals[0], klingons);
    assert_eq!(totals[2], bases);

    let chart = output_values(writer.text(), "CHART ");
    assert_eq!(chart.len(), 64);
    let observed = chart.iter().filter(|value| **value != 0).count();
    assert!((4..=9).contains(&observed));
    assert_eq!(chart[40], 8);
    assert_eq!(chart[41], 4);
    assert_eq!(chart[48], 302);
    assert_eq!(chart[49], 7);
    assert_eq!(chart[56], 1);
    assert_eq!(chart[57], 5);
    assert_eq!(output_values(writer.text(), "DAMAGE "), vec![0; 8]);
    assert_eq!(
        output_values(writer.text(), "COURSE_DX "),
        [10, 7, 0, -7, -10, -7, 0, 7, 10]
    );
    assert_eq!(
        output_values(writer.text(), "COURSE_DY "),
        [0, -7, -10, -7, 0, 7, 10, 7, 0]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_start_game_sets_docking_and_condition_before_the_first_command() {
    let original = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 entry point should be readable");
    let game_path = example_path("sttr1/game.tbx");
    let game = std::fs::read_to_string(game_path).expect("STTR1 game source should be readable");
    let scenarios = [
        ("DOCK", 1, 1, [1, 3000, 10, 0, 3]),
        ("RED", 0, 1, [0, 3000, 10, 77, 2]),
        ("GREEN", 0, 0, [0, 3000, 10, 77, 0]),
    ];

    for (name, docked, klingons, expected) in scenarios {
        let setup = format!(
            "  INIT_QUADRANT\n  LET ENT_SX = 4\n  LET ENT_SY = 4\n  LET I = 1\n  WHILE I <= 64\n    LET @SECTOR[I] = 0\n    LET I = I + 1\n  ENDWH\n  LET KLINGONS_HERE = {klingons}\n  LET DOCKED = 0\n  LET ENERGY = 3000\n  LET TORPEDOES = 10\n  LET SHIELDS = 77\n{}\nEND\n\nDEF PRINT_BRIEFING",
            if docked == 1 { "  LET @SECTOR[27] = 3" } else { "" }
        );
        let scenario_game = game.replace("  INIT_QUADRANT\nEND\n\nDEF PRINT_BRIEFING", &setup);
        let scenario_game = scenario_game.replace(
            "  GAME_LOOP\nEND\n",
            "  PRINT \"INITIAL_STATE \", DOCKED, \" \", ENERGY, \" \", TORPEDOES, \" \", SHIELDS, \" \", CONDITION\n  CR\n  CHECK_DOCKING\n  CHECK_DOCKING\n  PRINT \"REPEATED_STATE \", DOCKED, \" \", ENERGY, \" \", TORPEDOES, \" \", SHIELDS, \" \", CONDITION, \" \", RND(200)\n  CR\nEND\n",
        );
        let source = original.replace("USE \"game.tbx\"", &scenario_game);
        // Keep the real START_GAME entry point while replacing only its interactive loop.
        let mut sources = SourceTexts::new();
        let standard_library_id = sources.register(STDLIB_SOURCE, "<tbx-next-stdlib>");
        let canonical_path = std::fs::canonicalize(example_path("sttr1/main.tbx"))
            .expect("STTR1 entry point should have a canonical filesystem path");
        let source_id = sources.register_with_acquisition(
            source.as_str(),
            "docs/next/examples/sttr1/main.tbx",
            crate::source::SourceAcquisition::FileSystem { canonical_path },
        );
        let mut writer = RecordingWriter::default();
        let result = success(execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut writer,
            None,
            30,
        ));

        assert_eq!(
            output_values(writer.text(), "INITIAL_STATE "),
            expected,
            "{name}"
        );
        assert_eq!(
            &output_values(writer.text(), "REPEATED_STATE ")[..5],
            &expected,
            "{name} repeated check"
        );
        if docked == 1 {
            assert_eq!(
                writer
                    .text()
                    .matches("DOCKED: ENERGY AND TORPEDOES REPLENISHED; SHIELDS RESET TO 0")
                    .count(),
                1
            );
        } else {
            assert!(!writer
                .text()
                .contains("DOCKED: ENERGY AND TORPEDOES REPLENISHED"));
        }
        assert_eq!(result.data_stack(), [], "{name}");

        let control_game = scenario_game.replace(
            "  INIT_MISSION\n  CHECK_DOCKING\n  PRINT_BRIEFING",
            "  INIT_MISSION\n  PRINT_BRIEFING",
        );
        let control_source = original.replace("USE \"game.tbx\"", &control_game);
        let mut control_sources = SourceTexts::new();
        let control_stdlib_id = control_sources.register(STDLIB_SOURCE, "<tbx-next-stdlib>");
        let control_source_id = control_sources.register_with_acquisition(
            control_source.as_str(),
            "docs/next/examples/sttr1/main.tbx",
            crate::source::SourceAcquisition::FileSystem {
                canonical_path: std::fs::canonicalize(example_path("sttr1/main.tbx"))
                    .expect("STTR1 entry point should have a canonical filesystem path"),
            },
        );
        let mut control_writer = RecordingWriter::default();
        success(execute_registered_sources_with_filesystem_and_seed(
            control_sources,
            control_stdlib_id,
            control_source_id,
            &mut control_writer,
            None,
            30,
        ));
        assert_eq!(
            output_values(writer.text(), "REPEATED_STATE ")[5],
            output_values(control_writer.text(), "REPEATED_STATE ")[5],
            "{name} startup docking must not consume random values"
        );
    }
}
