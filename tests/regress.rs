// SPDX-FileCopyrightText: Copyright Eric S. Raymond <esr@thyrsus.com>
// SPDX-License-Identifier: BSD-2-Clause
//
// Runs Open Adventure's regression suite against the Rust port.
//
// Each tests/regress/*.log is a command script; it is fed to `advent`
// on stdin (with any options from a "#options:" line) and the output
// must match the corresponding .chk file byte for byte.  Logs run in
// alphabetical order in a shared scratch directory, because some of
// them (saveresume.*) depend on files written by earlier ones.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const ADVENT: &str = env!("CARGO_BIN_EXE_advent");
const CHEAT: &str = env!("CARGO_BIN_EXE_cheat");

fn setup(name: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/regress");
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&root);
    let work = root.join("tests");
    fs::create_dir_all(&work).unwrap();
    for entry in fs::read_dir(&src).unwrap() {
        let p = entry.unwrap().path();
        fs::copy(&p, work.join(p.file_name().unwrap())).unwrap();
    }
    // badmagic.log resumes from "../main.o", which in the C tree is an
    // object file: anything that isn't a save file will do.
    fs::write(root.join("main.o"), b"\x7fELF this is not a save file").unwrap();
    work
}

fn run_advent(dir: &Path, args: &[&str], stdin: &[u8]) -> Vec<u8> {
    let mut child = Command::new(ADVENT)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn advent");
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    let mut v = out.stdout;
    v.extend_from_slice(&out.stderr);
    v
}

fn cheat(dir: &Path, args: &[&str]) {
    let st = Command::new(CHEAT)
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::null())
        .status()
        .expect("spawn cheat");
    assert!(st.success(), "cheat {args:?} failed");
}

fn first_diff(expected: &[u8], actual: &[u8]) -> String {
    let e = String::from_utf8_lossy(expected);
    let a = String::from_utf8_lossy(actual);
    for (n, (el, al)) in e.lines().zip(a.lines()).enumerate() {
        if el != al {
            return format!("line {}:\n  expected: {el:?}\n  actual:   {al:?}", n + 1);
        }
    }
    format!(
        "lengths differ: expected {} lines, got {}",
        e.lines().count(),
        a.lines().count()
    )
}

#[test]
fn regression_logs() {
    let work = setup("regress");

    // Generate bogus savegames.
    cheat(&work, &["-d", "-900", "-o", "cheat_numdie.adv"]);
    cheat(&work, &["-d", "-1000", "-o", "cheat_numdie1000.adv"]);
    cheat(&work, &["-d", "2000", "-o", "cheat_savetamper.adv"]);
    cheat(&work, &["-v", "-1337", "-o", "resume_badversion.adv"]);
    cheat(&work, &["-s", "-1000", "-o", "thousand_saves.adv"]);
    cheat(&work, &["-t", "-1000", "-o", "thousand_turns.adv"]);
    cheat(&work, &["-l", "-1000", "-o", "thousand_limit.adv"]);

    let mut logs: Vec<String> = fs::read_dir(&work)
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".log").map(str::to_string)
        })
        .collect();
    logs.sort();
    assert!(logs.len() > 100, "regression logs missing");

    let mut failures = Vec::new();
    for stem in &logs {
        let log = fs::read(work.join(format!("{stem}.log"))).unwrap();
        let text = String::from_utf8_lossy(&log);
        let opts: Vec<&str> = text
            .lines()
            .find_map(|l| l.split_once("#options:").map(|(_, o)| o))
            .map(|o| o.split_whitespace().collect())
            .unwrap_or_default();
        let expected = fs::read(work.join(format!("{stem}.chk"))).unwrap();
        let actual = run_advent(&work, &opts, &log);
        if actual != expected {
            failures.push(format!("{stem}: {}", first_diff(&expected, &actual)));
        }
    }

    // Multiple script files on the command line.
    let actual = run_advent(&work, &["issue36.log", "/dev/stdin"], b"inven\n");
    let expected = fs::read(work.join("multifile.chk")).unwrap();
    if actual != expected {
        failures.push(format!("multifile: {}", first_diff(&expected, &actual)));
    }

    assert!(
        failures.is_empty(),
        "{} of {} regression tests failed:\n{}",
        failures.len(),
        logs.len() + 1,
        failures.join("\n")
    );
}

#[test]
fn bad_option_files_are_survivable() {
    let work = setup("options");
    let pitfall = fs::read(work.join("pitfall.log")).unwrap();

    // Doesn't start with invalid file with -r; still plays the game.
    let out = run_advent(&work, &["-r", "/badfilename"], &pitfall);
    let out = String::from_utf8_lossy(&out);
    assert!(out.contains("can't open save file /badfilename"));
    assert!(out.contains("You scored"));

    // Unwritable log file is reported but not fatal.
    let out = run_advent(&work, &["-l", "/"], &pitfall);
    let out = String::from_utf8_lossy(&out);
    assert!(out.contains("can't open logfile / for write"));
    assert!(out.contains("You scored"));

    // A valid save restores.
    cheat(&work, &["-s", "-1000", "-o", "thousand_saves.adv"]);
    let out = run_advent(&work, &["-r", "thousand_saves.adv"], &pitfall);
    assert!(String::from_utf8_lossy(&out).contains("You scored"));
}

#[test]
fn cheat_requires_output_file() {
    let st = Command::new(CHEAT).stderr(Stdio::null()).status().unwrap();
    assert!(!st.success());
    let st = Command::new(CHEAT)
        .args(["-Q"])
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!st.success());
}

#[test]
fn usage_on_bad_option() {
    let out = Command::new(ADVENT)
        .arg("-x")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage:"));
}
