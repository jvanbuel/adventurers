// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause

//! Plays the game in-process through its public interface: an
//! [`Engine`] driven by a [`ScriptedConsole`].

use adventurers::save::{NewSave, write_new_game};
use adventurers::{Config, Engine, GameOver, ScriptedConsole, Start};

/// Play a fresh game with a fixed seed.
fn play(lines: &[&str]) -> (GameOver, String) {
    let mut engine = Engine::new(
        ScriptedConsole::new(lines.iter().copied()),
        Config::default(),
        1,
    );
    let over = engine.play(Start::New);
    (over, engine.console().transcript().to_owned())
}

#[test]
fn quitting_shows_the_score() {
    let (over, transcript) = play(&["no", "quit", "yes"]);
    assert!(matches!(over, GameOver::Finished { .. }));
    assert!(transcript.contains("You scored"));
}

#[test]
fn running_out_of_input_at_a_question_ends_quietly() {
    let (over, transcript) = play(&[]);
    assert_eq!(over, GameOver::InputExhausted);
    assert!(!transcript.contains("You scored"));
}

#[test]
fn running_out_of_commands_scores_as_quitting() {
    let (over, _) = play(&["no", "in"]);
    let GameOver::Finished { score } = over else {
        panic!("expected a finished game, got {over:?}");
    };
    // Not dead, not deep, not closed: survival plus the round-off.
    assert_eq!(score, 32);
}

#[test]
fn asking_for_instructions_costs_points() {
    let (with, _) = play(&["yes"]);
    let (without, _) = play(&["no"]);
    let (GameOver::Finished { score: with }, GameOver::Finished { score: without }) =
        (with, without)
    else {
        panic!("both games should finish");
    };
    assert_eq!(without - with, 5);
}

#[test]
fn two_word_commands_and_object_first_order() {
    let (_, transcript) = play(&["no", "in", "lamp take", "inventory"]);
    assert!(transcript.contains("> lamp take\n\nOK\n"));
    assert!(transcript.contains("Brass lantern"));
}

#[test]
fn a_bare_verb_asks_for_its_object() {
    let (_, transcript) = play(&["no", "in", "get", "food"]);
    assert!(transcript.contains("> get\n\nGet what?\n"));
    assert!(transcript.contains("> food\n\nOK\n"));
}

#[test]
fn the_same_seed_plays_the_same_game() {
    let lines = ["no", "in", "take lamp", "xyzzy", "on", "w", "w", "listen"];
    assert_eq!(play(&lines).1, play(&lines).1);
}

#[test]
fn saving_and_restoring() {
    let dir = std::env::temp_dir().join(format!("adventurers-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("game.adv");
    let path_str = path.to_str().unwrap();

    let (over, _) = play(&["no", "in", "take lamp", "save", "yes", path_str]);
    assert_eq!(over, GameOver::Saved);

    let bytes = std::fs::read(&path).unwrap();
    let console = ScriptedConsole::new(["inventory"]);
    let mut engine = Engine::new(console, Config::default(), 2);
    let over = engine.play(Start::Restore(bytes));
    let transcript = engine.console().transcript();
    assert!(transcript.contains("Brass lantern"), "{transcript}");
    let GameOver::Finished { score } = over else {
        panic!("expected a finished game, got {over:?}");
    };
    // Saving cost five points.
    assert_eq!(score, 32 - 5);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_tampered_save_ends_the_game() {
    let mut bytes = Vec::new();
    let opts = NewSave {
        deaths: 1000,
        ..NewSave::default()
    };
    write_new_game(&mut bytes, &opts).unwrap();
    let mut engine = Engine::new(ScriptedConsole::new(["look"]), Config::default(), 1);
    assert_eq!(engine.play(Start::Restore(bytes)), GameOver::TamperedSave);
}

#[test]
fn saving_can_be_disabled() {
    let config = Config {
        saves: adventurers::SaveMode::Disabled,
        ..Config::default()
    };
    let mut engine = Engine::new(ScriptedConsole::new(["no", "save"]), config, 1);
    let over = engine.play(Start::New);
    assert!(matches!(over, GameOver::Finished { .. }));
}

#[test]
fn oldstyle_has_no_prompt_and_shouts() {
    let config = Config {
        oldstyle: true,
        ..Config::default()
    };
    let mut engine = Engine::new(ScriptedConsole::new(["n", "i"]), config, 1);
    let _ = engine.play(Start::New);
    let transcript = engine.console().transcript();
    assert!(transcript.starts_with("Initialising...\n"));
    assert!(transcript.contains("\ni\n"));
    assert!(transcript.contains("\"I\""));
}

/// Killing the bird while it sits in its cage, then taking the cage,
/// made the C version loop forever.
#[test]
fn taking_the_cage_after_killing_the_caged_bird() {
    let lines = [
        "no",
        "in",
        "take lamp",
        "xyzzy",
        "on",
        "e",
        "take cage",
        "w",
        "w",
        "w",
        "take bird",
        "drop cage",
        "kill bird",
        "take cage",
        "inventory",
    ];
    let (over, transcript) = play(&lines);
    assert!(matches!(over, GameOver::Finished { .. }));
    let after_kill = &transcript[transcript.find("> kill bird").unwrap()..];
    assert!(after_kill.contains("> take cage\n\nOK\n"), "{after_kill}");
}
