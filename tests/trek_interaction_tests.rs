use std::io::Cursor;
use std::path::PathBuf;

use tbx::interpreter::Interpreter;
use tbx::vm::InputFlushMode;

fn run_trek_interaction(src: &str, input: &str) -> String {
    let mut interp = Interpreter::new();
    interp
        .set_base_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/trek"))
        .expect("examples/trek path is absolute");
    interp.vm_mut().input_reader = Box::new(Cursor::new(input.to_string()));
    interp
        .vm_mut()
        .set_input_flush_mode(InputFlushMode::KeepBufferedForTest);
    interp
        .exec_source(src)
        .unwrap_or_else(|e| panic!("trek interaction test failed: {e}"));
    interp.take_output()
}

#[test]
fn test_read_number_or_keeps_command_prompt_buffered() {
    let src = concat!(
        "USE \"state.tbx\"\n",
        "USE \"util.tbx\"\n",
        "USE \"../../lib/tests/helper.tbx\"\n",
        "DEF RUN()\n",
        "  PUTSTR \"COMMAND:\"\n",
        "  VAR CMD = READ_NUMBER_OR(-1)\n",
        "  ASSERT (CMD = 7)\n",
        "  ASSERT_OUTPUT \"COMMAND:\"\n",
        "END\n",
        "RUN\n",
    );

    let out = run_trek_interaction(src, "7\n");
    assert_eq!(out, "");
}

#[test]
fn test_library_computer_invalid_input_keeps_prompt_and_menu_buffered() {
    let src = concat!(
        "USE \"state.tbx\"\n",
        "USE \"util.tbx\"\n",
        "USE \"init.tbx\"\n",
        "USE \"scan.tbx\"\n",
        "USE \"combat.tbx\"\n",
        "USE \"nav.tbx\"\n",
        "USE \"library.tbx\"\n",
        "USE \"../../lib/tests/helper.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  LIBRARY_COMPUTER\n",
        "  ASSERT_OUTPUT \"COMPUTER ACTIVE AND AWAITING COMMANDFUNCTIONS AVAILABLE FROM COMPUTER\\n   0 = CUMULATIVE GALACTIC RECORD\\n   1 = STATUS REPORT\\n   2 = PHOTON TORPEDO DATA\\n\"\n",
        "END\n",
        "RUN\n",
    );

    let out = run_trek_interaction(src, "9\n");
    assert_eq!(out, "");
}

#[test]
fn test_dispatch_command_7_invalid_input_keeps_prompt_and_menu_buffered() {
    let src = concat!(
        "USE \"state.tbx\"\n",
        "USE \"util.tbx\"\n",
        "USE \"init.tbx\"\n",
        "USE \"scan.tbx\"\n",
        "USE \"combat.tbx\"\n",
        "USE \"nav.tbx\"\n",
        "USE \"library.tbx\"\n",
        "USE \"command.tbx\"\n",
        "USE \"../../lib/tests/helper.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  DISPATCH_COMMAND 7\n",
        "  ASSERT_OUTPUT \"COMPUTER ACTIVE AND AWAITING COMMANDFUNCTIONS AVAILABLE FROM COMPUTER\\n   0 = CUMULATIVE GALACTIC RECORD\\n   1 = STATUS REPORT\\n   2 = PHOTON TORPEDO DATA\\n\"\n",
        "END\n",
        "RUN\n",
    );

    let out = run_trek_interaction(src, "9\n");
    assert_eq!(out, "");
}

#[test]
fn test_trek_command_loop_refreshes_docking_after_navigation_before_next_prompt() {
    let src = concat!(
        "USE \"state.tbx\"\n",
        "USE \"util.tbx\"\n",
        "USE \"init.tbx\"\n",
        "USE \"scan.tbx\"\n",
        "USE \"combat.tbx\"\n",
        "USE \"nav.tbx\"\n",
        "USE \"library.tbx\"\n",
        "USE \"command.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  CLEAR_SECTOR\n",
        "  LET ENT_SX = 4\n",
        "  LET ENT_SY = 4\n",
        "  LET @SECTOR[ENT_SX, ENT_SY] = 1\n",
        "  LET @SECTOR[6, 4] = 3\n",
        "  LET DOCKED = FALSE\n",
        "  LET CONDITION = \"GREEN\"\n",
        "  LET ENERGY = MAX_ENERGY - 100\n",
        "  LET TORPEDOES = MAX_TORPEDOES - 2\n",
        "  LET SHIELDS = 250\n",
        "  LET KLINGONS_HERE = 0\n",
        "  LET START_STARDATE = 2000\n",
        "  LET STARDATE = 2000\n",
        "  LET MISSION_DAYS = 0\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  RUN_COMMAND_LOOP\n",
        "END\n",
        "RUN\n",
    );

    let out = run_trek_interaction(src, "0\n1\n0.2\n0\n1\n1.0\n");
    assert_eq!(
        out.matches("SHIELDS DROPPED FOR DOCKING PURPOSES\n")
            .count(),
        1,
        "navigation should trigger exactly one docking refresh before the next prompt"
    );
}

#[test]
fn test_trek_command_loop_does_not_double_refresh_after_command_one_scan() {
    let src = concat!(
        "USE \"state.tbx\"\n",
        "USE \"util.tbx\"\n",
        "USE \"init.tbx\"\n",
        "USE \"scan.tbx\"\n",
        "USE \"combat.tbx\"\n",
        "USE \"nav.tbx\"\n",
        "USE \"library.tbx\"\n",
        "USE \"command.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  CLEAR_SECTOR\n",
        "  LET ENT_SX = 4\n",
        "  LET ENT_SY = 4\n",
        "  LET @SECTOR[ENT_SX, ENT_SY] = 1\n",
        "  LET @SECTOR[5, 4] = 3\n",
        "  LET DOCKED = FALSE\n",
        "  LET CONDITION = \"GREEN\"\n",
        "  LET ENERGY = MAX_ENERGY - 100\n",
        "  LET TORPEDOES = MAX_TORPEDOES - 2\n",
        "  LET SHIELDS = 250\n",
        "  LET KLINGONS_HERE = 0\n",
        "  LET START_STARDATE = 2000\n",
        "  LET STARDATE = 2000\n",
        "  LET MISSION_DAYS = 0\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  RUN_COMMAND_LOOP\n",
        "END\n",
        "RUN\n",
    );

    let out = run_trek_interaction(src, "1\n0\n1\n1.0\n");
    assert_eq!(
        out.matches("SHIELDS DROPPED FOR DOCKING PURPOSES\n").count(),
        2,
        "initial scan and command 1 scan should dock once each without an extra post-command refresh"
    );
}

#[test]
fn test_trek_command_loop_does_not_refresh_after_non_navigation_command() {
    let src = concat!(
        "USE \"state.tbx\"\n",
        "USE \"util.tbx\"\n",
        "USE \"init.tbx\"\n",
        "USE \"scan.tbx\"\n",
        "USE \"combat.tbx\"\n",
        "USE \"nav.tbx\"\n",
        "USE \"library.tbx\"\n",
        "USE \"command.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  CLEAR_SECTOR\n",
        "  LET ENT_SX = 4\n",
        "  LET ENT_SY = 4\n",
        "  LET @SECTOR[ENT_SX, ENT_SY] = 1\n",
        "  LET @SECTOR[5, 4] = 3\n",
        "  LET DOCKED = TRUE\n",
        "  LET CONDITION = \"DOCKED\"\n",
        "  LET SHIELDS = 200\n",
        "  LET KLINGONS_HERE = 0\n",
        "  LET START_STARDATE = 2000\n",
        "  LET STARDATE = 2000\n",
        "  LET MISSION_DAYS = 0\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  RUN_COMMAND_LOOP\n",
        "  PUTDEC SHIELDS\n",
        "END\n",
        "RUN\n",
    );

    let out = run_trek_interaction(src, "5\n123\n0\n1\n1.0\n");
    assert_eq!(
        out.matches("SHIELDS DROPPED FOR DOCKING PURPOSES\n").count(),
        1,
        "only the initial short-range scan should refresh docking for a non-navigation command cycle"
    );
    assert!(
        out.ends_with("123"),
        "shield control value should survive until loop exit without post-command reset"
    );
}

#[test]
fn test_navigate_invalid_course_does_not_repair_damage() {
    let src = concat!(
        "USE \"state.tbx\"\n",
        "USE \"util.tbx\"\n",
        "USE \"init.tbx\"\n",
        "USE \"scan.tbx\"\n",
        "USE \"combat.tbx\"\n",
        "USE \"nav.tbx\"\n",
        "USE \"../../lib/tests/helper.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  SET_DEVICE_DAMAGE DAMAGE_SLOT_PHASER_CONTROL(), -3\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  NAVIGATE\n",
        "  ASSERT (@DAMAGE[DAMAGE_SLOT_PHASER_CONTROL()] = -3)\n",
        "END\n",
        "RUN\n",
    );

    let _out = run_trek_interaction(src, "0\n");
}

#[test]
fn test_navigate_invalid_warp_does_not_repair_damage() {
    let src = concat!(
        "USE \"state.tbx\"\n",
        "USE \"util.tbx\"\n",
        "USE \"init.tbx\"\n",
        "USE \"scan.tbx\"\n",
        "USE \"combat.tbx\"\n",
        "USE \"nav.tbx\"\n",
        "USE \"../../lib/tests/helper.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  SET_DEVICE_DAMAGE DAMAGE_SLOT_PHASER_CONTROL(), -3\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  NAVIGATE\n",
        "  ASSERT (@DAMAGE[DAMAGE_SLOT_PHASER_CONTROL()] = -3)\n",
        "END\n",
        "RUN\n",
    );

    let _out = run_trek_interaction(src, "1\n9\n");
}
