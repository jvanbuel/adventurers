// SPDX-FileCopyrightText: (C) Eric S. Raymond <esr@thyrsus.com>
// SPDX-License-Identifier: BSD-2-Clause
//
// The dungeon compiler.  This is a Rust port of make_dungeon.py from
// Open Adventure.  It consumes the YAML description of the dungeon in
// adventure.yaml and writes $OUT_DIR/dungeon.rs, which src/dungeon.rs
// includes.
//
// The nontrivial part of this is the compilation of the YAML for
// movement rules into the travel array that's actually used by
// playermove().

use serde_yaml::Value;
use std::collections::HashMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const YAML_NAME: &str = "adventure.yaml";

/// Condition-bit names, in bit order (see the COND_* constants in dungeon.rs).
const COND_NAMES: &[&str] = &[
    "LIT",
    "OILY",
    "FLUID",
    "NOARRR",
    "NOBACK",
    "ABOVE",
    "DEEP",
    "FOREST",
    "FORCED",
    "ALLDIFFERENT",
    "ALLALIKE",
    "HBASE",
    "HCAVE",
    "HBIRD",
    "HSNAKE",
    "HMAZE",
    "HDARK",
    "HWITT",
    "HCLIFF",
    "HWOODS",
    "HOGRE",
    "HJADE",
];

fn fail(msg: &str) -> ! {
    panic!("dungeon: {msg}");
}

fn key_name(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Value::Number(n) => n.to_string(),
        other => fail(&format!("unexpected key {other:?}")),
    }
}

/// An `!!omap` (after tag stripping) is a sequence of single-entry maps.
/// Return it as an ordered list of (name, body) pairs.
fn omap(v: &Value) -> Vec<(String, Value)> {
    let seq = v.as_sequence().unwrap_or_else(|| fail("expected omap"));
    seq.iter()
        .map(|item| {
            let m = item.as_mapping().unwrap_or_else(|| fail("bad omap entry"));
            let (k, v) = m.iter().next().unwrap_or_else(|| fail("empty omap entry"));
            (key_name(k), v.clone())
        })
        .collect()
}

fn get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v.get(key) {
        None | Some(Value::Null) => None,
        Some(x) => Some(x),
    }
}

/// The C generator pastes YAML strings into C string literals without
/// escaping backslashes, so the C compiler interprets sequences such as
/// `\n` that appear literally in single-quoted YAML.  Do the same.
fn c_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => c_unescape(s),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => fail(&format!("expected string, got {other:?}")),
    }
}

fn int_of(v: &Value) -> i64 {
    v.as_i64()
        .unwrap_or_else(|| fail(&format!("expected integer, got {v:?}")))
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64() != Some(0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn opt_str(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "None".to_string(),
        Some(x) => format!("Some({:?})", str_of(x)),
    }
}

fn str_list(v: Option<&Value>) -> Vec<String> {
    match v {
        None | Some(Value::Null) => vec![],
        Some(Value::Sequence(s)) => s.iter().map(str_of).collect(),
        Some(other) => fail(&format!("expected list, got {other:?}")),
    }
}

/// A list of possibly-null strings; a missing list is rendered as a
/// single None entry, mirroring the C generator's `{NULL,}`.
fn opt_str_list(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "&[None]".to_string(),
        Some(Value::Sequence(s)) => {
            let items: Vec<String> = s
                .iter()
                .map(|x| match x {
                    Value::Null => "None".to_string(),
                    other => format!("Some({:?})", str_of(other)),
                })
                .collect();
            format!("&[{}]", items.join(", "))
        }
        Some(other) => fail(&format!("expected list, got {other:?}")),
    }
}

fn words_literal(v: Option<&Value>) -> String {
    let w = str_list(v);
    let items: Vec<String> = w.iter().map(|s| format!("{s:?}")).collect();
    format!("&[{}]", items.join(", "))
}

fn index_of(names: &[String], name: &str, what: &str) -> i64 {
    names
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| fail(&format!("unknown {what} {name}"))) as i64
}

struct TravelRow {
    motion: i64,
    condtype: &'static str,
    condarg1: i64,
    condarg2: i64,
    desttype: &'static str,
    destval: i64,
    nodwarves: bool,
    stop: bool,
}

fn main() {
    println!("cargo:rerun-if-changed={YAML_NAME}");
    println!("cargo:rerun-if-changed=build.rs");

    let text = fs::read_to_string(YAML_NAME).unwrap_or_else(|e| fail(&format!("{YAML_NAME}: {e}")));
    // The YAML relies on a couple of PyYAML conveniences: !!omap tags
    // and bare `!!null` scalars.  Normalise these so a YAML 1.2
    // parser is happy; an omap is just a sequence of one-entry maps.
    let text = text.replace("!!omap", "").replace("!!null", "null");
    let db: Value = serde_yaml::from_str(&text).unwrap_or_else(|e| fail(&format!("{e}")));

    let motions = omap(&db["motions"]);
    let actions = omap(&db["actions"]);
    let locations = omap(&db["locations"]);
    let objects = omap(&db["objects"]);
    let messages = omap(&db["arbitrary_messages"]);
    let hints = db["hints"].as_sequence().unwrap_or_else(|| fail("hints"));
    let classes = db["classes"]
        .as_sequence()
        .unwrap_or_else(|| fail("classes"));
    let thresholds = db["turn_thresholds"]
        .as_sequence()
        .unwrap_or_else(|| fail("turn_thresholds"));
    let obituaries = db["obituaries"]
        .as_sequence()
        .unwrap_or_else(|| fail("obituaries"));
    let dwarflocs: Vec<String> = str_list(Some(&db["dwarflocs"]));

    let locnames: Vec<String> = locations.iter().map(|(n, _)| n.clone()).collect();
    let msgnames: Vec<String> = messages.iter().map(|(n, _)| n.clone()).collect();
    let objnames: Vec<String> = objects.iter().map(|(n, _)| n.clone()).collect();
    let motionnames: Vec<String> = motions.iter().map(|(n, _)| n.clone()).collect();

    assert!(locations.len() <= 300);
    assert!(objects.len() <= 100);

    let mut out = String::new();
    out.push_str("// Generated from adventure.yaml by build.rs - do not hand-hack!\n\n");

    // ---- Symbolic constants ------------------------------------------
    let _ = writeln!(out, "pub const NLOCATIONS: usize = {};", locations.len());
    let _ = writeln!(out, "pub const NOBJECTS: usize = {};", objects.len());
    let _ = writeln!(out, "pub const NHINTS: usize = {};", hints.len());
    let _ = writeln!(out, "pub const NCLASSES: usize = {};", classes.len());
    let _ = writeln!(out, "pub const NDEATHS: usize = {};", obituaries.len());
    let _ = writeln!(out, "pub const NTHRESHOLDS: usize = {};", thresholds.len());
    let _ = writeln!(out, "pub const NMOTIONS: usize = {};", motions.len());
    let _ = writeln!(out, "pub const NACTIONS: usize = {};", actions.len());
    let _ = writeln!(out, "pub const NDWARVES: usize = {};", dwarflocs.len());

    let emit_refs = |out: &mut String, title: &str, names: &[String]| {
        let _ = writeln!(out, "\n// {title}");
        for (i, n) in names.iter().enumerate() {
            // Only names that are valid Rust identifiers become constants.
            if n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && n.chars().next().is_some_and(|c| !c.is_ascii_digit())
            {
                let _ = writeln!(out, "pub const {n}: i32 = {i};");
            }
        }
    };
    emit_refs(&mut out, "Arbitrary messages", &msgnames);
    emit_refs(&mut out, "Locations", &locnames);
    emit_refs(&mut out, "Objects", &objnames);
    emit_refs(&mut out, "Motions", &motionnames);
    let actnames: Vec<String> = actions.iter().map(|(n, _)| n.clone()).collect();
    emit_refs(&mut out, "Actions", &actnames);

    // ---- Object state definitions -----------------------------------
    out.push_str("\n// Object states\n");
    let mut max_state = 0i64;
    for (name, attr) in &objects {
        if get(attr, "descriptions").is_none() {
            continue;
        }
        let labels = str_list(get(attr, "states"));
        if !labels.is_empty() {
            let _ = writeln!(out, "// States for {name}");
            for (n, label) in labels.iter().enumerate() {
                let _ = writeln!(out, "pub const {label}: i32 = {n};");
                max_state = max_state.max(n as i64);
            }
        }
    }
    let _ = writeln!(
        out,
        "/// Maximum state value\npub const MAX_STATE: i32 = {max_state};"
    );

    // 0-origin index of bird's last song.  Bird should die after
    // player hears this.
    let bird = objects
        .iter()
        .find(|(n, _)| n == "BIRD")
        .unwrap_or_else(|| fail("no BIRD object"));
    let deathbird = str_list(get(&bird.1, "sounds")).len() as i64 - 1;
    let _ = writeln!(out, "pub const BIRD_ENDSTATE: i32 = {deathbird};");

    // ---- Arbitrary messages ------------------------------------------
    let _ = writeln!(
        out,
        "\npub static ARBITRARY_MESSAGES: [Option<&str>; {}] = [",
        messages.len()
    );
    for (name, body) in &messages {
        let _ = writeln!(out, "    {}, // {name}", opt_str(Some(body)));
    }
    out.push_str("];\n");

    // ---- Classes -----------------------------------------------------
    let _ = writeln!(out, "\npub static CLASSES: [Class; {}] = [", classes.len());
    for c in classes {
        let _ = writeln!(
            out,
            "    Class {{ threshold: {}, message: {} }},",
            int_of(&c["threshold"]),
            opt_str(get(c, "message"))
        );
    }
    out.push_str("];\n");

    // ---- Turn thresholds ----------------------------------------------
    let _ = writeln!(
        out,
        "\npub static TURN_THRESHOLDS: [TurnThreshold; {}] = [",
        thresholds.len()
    );
    for t in thresholds {
        let _ = writeln!(
            out,
            "    TurnThreshold {{ threshold: {}, point_loss: {}, message: {} }},",
            int_of(&t["threshold"]),
            int_of(&t["point_loss"]),
            opt_str(get(t, "message"))
        );
    }
    out.push_str("];\n");

    // ---- Locations ------------------------------------------------------
    let _ = writeln!(
        out,
        "\npub static LOCATIONS: [Location; {}] = [",
        locations.len()
    );
    for (i, (name, loc)) in locations.iter().enumerate() {
        let desc = &loc["description"];
        let sound = match get(loc, "sound") {
            None => -1,
            Some(s) => {
                let s = str_of(s);
                if s == "SILENT" {
                    -1
                } else {
                    index_of(&msgnames, &s, "message")
                }
            }
        };
        let _ = writeln!(
            out,
            "    Location {{ // {i}: {name}\n        small: {},\n        big: {},\n        sound: {sound},\n        loud: {},\n    }},",
            opt_str(get(desc, "short")),
            opt_str(get(desc, "long")),
            truthy(get(loc, "loud")),
        );
    }
    out.push_str("];\n");

    // ---- Objects ----------------------------------------------------------
    let _ = writeln!(out, "\npub static OBJECTS: [Object; {}] = [", objects.len());
    for (i, (name, attr)) in objects.iter().enumerate() {
        let immovable = truthy(get(attr, "immovable"));
        let (plac, fixd) = match get(attr, "locations") {
            None => (0, 0),
            Some(Value::Sequence(s)) => (
                index_of(&locnames, &str_of(&s[0]), "location"),
                index_of(&locnames, &str_of(&s[1]), "location"),
            ),
            Some(v) => (
                index_of(&locnames, &str_of(v), "location"),
                if immovable { -1 } else { 0 },
            ),
        };
        let _ = writeln!(
            out,
            "    Object {{ // {i}: {name}\n        words: {},\n        inventory: {},\n        plac: {plac},\n        fixd: {fixd},\n        is_treasure: {},\n        descriptions: {},\n        sounds: {},\n        texts: {},\n        changes: {},\n    }},",
            words_literal(get(attr, "words")),
            opt_str(get(attr, "inventory")),
            truthy(get(attr, "treasure")),
            opt_str_list(get(attr, "descriptions")),
            opt_str_list(get(attr, "sounds")),
            opt_str_list(get(attr, "texts")),
            opt_str_list(get(attr, "changes")),
        );
    }
    out.push_str("];\n");

    // ---- Obituaries -------------------------------------------------------
    let _ = writeln!(
        out,
        "\npub static OBITUARIES: [Obituary; {}] = [",
        obituaries.len()
    );
    for o in obituaries {
        let _ = writeln!(
            out,
            "    Obituary {{ query: {}, yes_response: {} }},",
            opt_str(get(o, "query")),
            opt_str(get(o, "yes_response"))
        );
    }
    out.push_str("];\n");

    // ---- Hints ------------------------------------------------------------
    let _ = writeln!(out, "\npub static HINTS: [Hint; {}] = [", hints.len());
    for member in hints {
        let h = &member["hint"];
        let _ = writeln!(
            out,
            "    Hint {{ number: {}, penalty: {}, turns: {}, question: {}, hint: {} }},",
            int_of(&h["number"]),
            int_of(&h["penalty"]),
            int_of(&h["turns"]),
            opt_str(get(h, "question")),
            opt_str(get(h, "hint"))
        );
    }
    out.push_str("];\n");

    // ---- Condition bits ---------------------------------------------------
    let cond_bit = |name: &str| -> usize {
        COND_NAMES
            .iter()
            .position(|c| *c == name)
            .unwrap_or_else(|| fail(&format!("unknown condition {name}")))
    };
    let _ = writeln!(
        out,
        "\npub static CONDITIONS: [i32; {}] = [",
        locations.len()
    );
    for (name, loc) in &locations {
        let mut bits: i64 = 0;
        if let Some(Value::Mapping(conds)) = get(loc, "conditions") {
            for (k, v) in conds {
                if truthy(Some(v)) {
                    bits |= 1 << cond_bit(&key_name(k));
                }
            }
        }
        if let Some(Value::Sequence(hs)) = get(loc, "hints") {
            for h in hs {
                bits |= 1 << cond_bit(&format!("H{}", str_of(&h["name"])));
            }
        }
        let _ = writeln!(out, "    {bits}, // {name}");
    }
    out.push_str("];\n");

    // ---- Motions, actions and the "ignore" list ---------------------------
    let mut ignore = String::new();
    let _ = writeln!(out, "\npub static MOTIONS: [Motion; {}] = [", motions.len());
    for (name, m) in &motions {
        let _ = writeln!(
            out,
            "    Motion {{ words: {} }}, // {name}",
            words_literal(get(m, "words"))
        );
        if get(m, "oldstyle").is_some_and(|v| !truthy(Some(v))) {
            for w in str_list(get(m, "words")) {
                if w.len() == 1 {
                    ignore.push_str(&w.to_uppercase());
                }
            }
        }
    }
    out.push_str("];\n");

    let _ = writeln!(out, "\npub static ACTIONS: [Action; {}] = [", actions.len());
    for (name, a) in &actions {
        let _ = writeln!(
            out,
            "    Action {{ words: {}, message: {}, noaction: {} }}, // {name}",
            words_literal(get(a, "words")),
            opt_str(get(a, "message")),
            get(a, "noaction").is_some()
        );
        if get(a, "oldstyle").is_some_and(|v| !truthy(Some(v))) {
            for w in str_list(get(a, "words")) {
                if w.len() == 1 {
                    ignore.push_str(&w.to_uppercase());
                }
            }
        }
    }
    out.push_str("];\n");
    let _ = writeln!(out, "\npub static IGNORE: &str = {ignore:?};");

    // ---- Travel table -----------------------------------------------------
    let (travel, tkey) = buildtravel(
        &locations, &objects, &motions, &locnames, &msgnames, &objnames,
    );
    let _ = writeln!(out, "\npub const NTRAVEL: usize = {};", travel.len());
    let _ = writeln!(out, "pub const NKEYS: usize = {};", tkey.len());
    let keys: Vec<String> = tkey.iter().map(|k| k.to_string()).collect();
    let _ = writeln!(
        out,
        "\npub static TKEY: [i32; {}] = [{}];",
        tkey.len(),
        keys.join(", ")
    );
    let _ = writeln!(out, "\npub static TRAVEL: [TravelOp; {}] = [", travel.len());
    for t in &travel {
        let _ = writeln!(
            out,
            "    TravelOp {{ motion: {}, condtype: CondType::{}, condarg1: {}, condarg2: {}, desttype: DestType::{}, destval: {}, nodwarves: {}, stop: {} }},",
            t.motion, t.condtype, t.condarg1, t.condarg2, t.desttype, t.destval, t.nodwarves, t.stop
        );
    }
    out.push_str("];\n");

    // ---- Dwarf starting locations -----------------------------------------
    let dl: Vec<String> = dwarflocs
        .iter()
        .map(|l| index_of(&locnames, l, "location").to_string())
        .collect();
    let _ = writeln!(
        out,
        "\npub static DWARFLOCS: [i32; NDWARVES] = [{}];",
        dl.join(", ")
    );

    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("dungeon.rs");
    fs::write(dest, out).unwrap();
}

/// Compile the YAML travel rules into the runtime travel table.
///
/// This follows make_dungeon.py closely: first the rules are compiled
/// to the "section 3" form of the old FORTRAN database (location,
/// encoded destination, list of motions), then that is unpacked into
/// the travel array.  In the encoded destination Y, M=Y/1000 is the
/// condition and N=Y%1000 the destination:
///   N<=300        location to go to
///   300<N<=500    N-300 is a special-case motion
///   N>500         message N-500 is printed
///   M=0           unconditional
///   0<M<100       done with M% probability
///   M=100         unconditional, but forbidden to dwarves
///   100<M<=200    must be carrying object M-100
///   200<M<=300    must be carrying or in same room as M-200
///   300<M         prop(M%100) must not be (M-300)/100
fn buildtravel(
    locs: &[(String, Value)],
    objs: &[(String, Value)],
    motions: &[(String, Value)],
    locnames: &[String],
    msgnames: &[String],
    objnames: &[String],
) -> (Vec<TravelRow>, Vec<i64>) {
    let mut verbmap: HashMap<String, i64> = HashMap::new();
    for (i, (_, m)) in motions.iter().enumerate() {
        for w in str_list(get(m, "words")) {
            verbmap.insert(w.to_uppercase(), i as i64);
        }
    }

    let dencode = |action: &Value, name: &str| -> i64 {
        let a = action
            .as_sequence()
            .unwrap_or_else(|| fail(&format!("bad action in {name}")));
        match str_of(&a[0]).as_str() {
            "goto" => index_of(locnames, &str_of(&a[1]), "location in goto clause"),
            "special" => 300 + int_of(&a[1]),
            "speak" => 500 + index_of(msgnames, &str_of(&a[1]), "message in speak clause"),
            other => fail(&format!("unknown action {other} in {name}")),
        }
    };

    let cencode = |cond: Option<&Value>, name: &str| -> i64 {
        let Some(cond) = cond else { return 0 };
        let c = cond
            .as_sequence()
            .unwrap_or_else(|| fail(&format!("bad cond in {name}")));
        match str_of(&c[0]).as_str() {
            "nodwarves" => 100,
            "pct" => int_of(&c[1]),
            "carry" => 100 + index_of(objnames, &str_of(&c[1]), "object in carry clause"),
            "with" => 200 + index_of(objnames, &str_of(&c[1]), "object in with clause"),
            "not" => {
                let obj = index_of(objnames, &str_of(&c[1]), "object in not clause");
                let state = match &c[2] {
                    Value::Number(n) => n.as_i64().unwrap(),
                    s => {
                        let s = str_of(s);
                        let states = str_list(get(&objs[obj as usize].1, "states"));
                        states.iter().position(|x| *x == s).unwrap_or_else(|| {
                            fail(&format!(
                                "unmatched state symbol {s} in not clause of {name}"
                            ))
                        }) as i64
                    }
                };
                300 + obj + 100 * state
            }
            other => fail(&format!("unknown condition {other} in {name}")),
        }
    };

    // Section-3 form: (loc, encoded destination, motions...)
    let mut ltravel: Vec<(i64, i64, Vec<i64>)> = Vec::new();
    for (i, (name, loc)) in locs.iter().enumerate() {
        let Some(Value::Sequence(rules)) = loc.get("travel") else {
            continue;
        };
        for rule in rules {
            let dest = dencode(&rule["action"], name) + 1000 * cencode(get(rule, "cond"), name);
            let verbs = str_list(get(rule, "verbs"));
            let mut mots: Vec<i64> = verbs
                .iter()
                .map(|v| {
                    *verbmap
                        .get(&v.to_uppercase())
                        .unwrap_or_else(|| fail(&format!("unknown verb {v} in {name}")))
                })
                .collect();
            if mots.is_empty() {
                mots.push(1); // Magic dummy entry for null rules (HERE)
            }
            ltravel.push((i as i64, dest, mots));
        }
    }

    // Now unpack into the runtime format.
    let mut travel = vec![TravelRow {
        motion: 0,
        condtype: "Goto",
        condarg1: 0,
        condarg2: 0,
        desttype: "Goto",
        destval: 0,
        nodwarves: false,
        stop: false,
    }];
    let mut tkey: Vec<i64> = vec![0];
    let mut oldloc = 0;
    for (loc, newloc, mots) in ltravel {
        if loc != oldloc {
            tkey.push(travel.len() as i64);
            oldloc = loc;
        } else if let Some(last) = travel.last_mut() {
            last.stop = !last.stop;
        }
        for motion in mots {
            let cond = newloc / 1000;
            let nodwarves = cond == 100;
            let (condtype, condarg1, condarg2) = if cond == 0 {
                ("Goto", 0, 0)
            } else if cond < 100 {
                ("Pct", cond, 0)
            } else if cond == 100 {
                ("Goto", 100, 0)
            } else if cond <= 200 {
                ("Carry", cond - 100, 0)
            } else if cond <= 300 {
                ("With", cond - 200, 0)
            } else {
                ("Not", cond % 100, (cond - 300) / 100)
            };
            let dest = newloc % 1000;
            let (desttype, destval) = if dest <= 300 {
                ("Goto", dest)
            } else if dest > 500 {
                ("Speak", dest - 500)
            } else {
                ("Special", dest - 300)
            };
            travel.push(TravelRow {
                motion,
                condtype,
                condarg1,
                condarg2,
                desttype,
                destval,
                nodwarves,
                stop: false,
            });
        }
        travel.last_mut().unwrap().stop = true;
    }
    (travel, tkey)
}
