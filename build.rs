// SPDX-FileCopyrightText: (C) Eric S. Raymond <esr@thyrsus.com>
// SPDX-License-Identifier: BSD-2-Clause
//
//! The dungeon compiler, a port of Open Adventure's make_dungeon.py.
//!
//! It reads adventure.yaml and writes $OUT_DIR/dungeon.rs: typed
//! constants for every named location, object, message, motion and verb,
//! plus the static tables that src/dungeon.rs exposes.  Travel rules are
//! compiled per location into the form playermove() walks.

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

fn is_ident(name: &str) -> bool {
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.chars().next().is_some_and(|c| !c.is_ascii_digit())
}

fn index_of(names: &[String], name: &str, what: &str) -> i64 {
    names
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| fail(&format!("unknown {what} {name}"))) as i64
}

/// One compiled travel rule, before rendering.
struct TravelRow {
    motion: i64,
    cond: String,
    dest: String,
    /// Motion is HERE: the rule fires as soon as the player arrives.
    on_arrival: bool,
    nodwarves: bool,
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
    let seq = |key: &str| {
        db[key]
            .as_sequence()
            .unwrap_or_else(|| fail(&format!("{key} must be a list")))
    };
    let hints = seq("hints");
    let classes = seq("classes");
    let thresholds = seq("turn_thresholds");
    let obituaries = seq("obituaries");
    let dwarflocs: Vec<String> = str_list(Some(&db["dwarflocs"]));

    let names = |v: &[(String, Value)]| v.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    let locnames = names(&locations);
    let msgnames = names(&messages);
    let objnames = names(&objects);
    let motionnames = names(&motions);
    let actnames = names(&actions);

    assert!(locations.len() <= 300);
    assert!(objects.len() <= 100);

    let mut out = String::new();
    out.push_str("// Generated from adventure.yaml by build.rs - do not hand-hack!\n\n");

    // ---- Table sizes ----------------------------------------------------
    for (name, n) in [
        ("NLOCATIONS", locations.len()),
        ("NOBJECTS", objects.len()),
        ("NMESSAGES", messages.len()),
        ("NHINTS", hints.len()),
        ("NCLASSES", classes.len()),
        ("NDEATHS", obituaries.len()),
        ("NMOTIONS", motions.len()),
        ("NACTIONS", actions.len()),
        ("NDWARVES", dwarflocs.len()),
    ] {
        let _ = writeln!(out, "pub const {name}: usize = {n};");
    }

    // ---- Typed identifiers ------------------------------------------------
    let emit_ids = |out: &mut String, ty: &str, names: &[String]| {
        let _ = writeln!(out, "\n// {ty} identifiers");
        for (i, n) in names.iter().enumerate() {
            // Only names that are valid Rust identifiers become constants.
            if is_ident(n) {
                let _ = writeln!(out, "pub const {n}: {ty} = {ty}({i});");
            }
        }
    };
    emit_ids(&mut out, "Msg", &msgnames);
    emit_ids(&mut out, "Loc", &locnames);
    emit_ids(&mut out, "Obj", &objnames);
    emit_ids(&mut out, "Motion", &motionnames);
    emit_ids(&mut out, "Verb", &actnames);

    // ---- Object state values ----------------------------------------------
    out.push_str("\n// Object state values\n");
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
        "/// Largest object state value.\npub const MAX_STATE: i32 = {max_state};"
    );

    // Index of the bird's last song.  The bird dies after the player
    // hears it.
    let bird = objects
        .iter()
        .find(|(n, _)| n == "BIRD")
        .unwrap_or_else(|| fail("no BIRD object"));
    let deathbird = str_list(get(&bird.1, "sounds")).len() as i64 - 1;
    let _ = writeln!(out, "pub const BIRD_ENDSTATE: i32 = {deathbird};");

    // ---- Arbitrary messages -------------------------------------------------
    out.push_str("\nstatic MESSAGES: [Option<&str>; NMESSAGES] = [\n");
    for (name, body) in &messages {
        let _ = writeln!(out, "    {}, // {name}", opt_str(Some(body)));
    }
    out.push_str("];\n");

    // ---- Classes and turn thresholds ----------------------------------------
    out.push_str("\npub(crate) static CLASSES: [Class; NCLASSES] = [\n");
    for c in classes {
        let _ = writeln!(
            out,
            "    Class {{ threshold: {}, message: {} }},",
            int_of(&c["threshold"]),
            opt_str(get(c, "message"))
        );
    }
    out.push_str("];\n");

    let _ = writeln!(
        out,
        "\npub(crate) static TURN_THRESHOLDS: [TurnThreshold; {}] = [",
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

    // ---- Locations, with their condition bits and travel rules --------------
    let cond_bit = |name: &str| -> usize {
        COND_NAMES
            .iter()
            .position(|c| *c == name)
            .unwrap_or_else(|| fail(&format!("unknown condition {name}")))
    };
    let forced_bit = cond_bit("FORCED");
    let travel = buildtravel(
        &locations, &objects, &motions, &locnames, &msgnames, &objnames,
    );

    out.push_str("\nstatic LOCATIONS: [Location; NLOCATIONS] = [\n");
    for (i, (name, loc)) in locations.iter().enumerate() {
        let desc = &loc["description"];
        let sound = match get(loc, "sound").map(str_of) {
            None => "None".to_string(),
            Some(s) if s == "SILENT" => "None".to_string(),
            Some(s) => format!("Some(Msg({}))", index_of(&msgnames, &s, "message")),
        };

        let mut bits: u32 = 0;
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
        // A location whose first rule fires unconditionally on arrival
        // ("HERE") moves the player on without asking for a command.
        let rules = &travel[i];
        if get(desc, "long").is_some() && rules.first().is_some_and(|r| r.on_arrival) {
            bits |= 1 << forced_bit;
        }

        let _ = writeln!(out, "    Location {{ // {i}: {name}");
        let _ = writeln!(out, "        small: {},", opt_str(get(desc, "short")));
        let _ = writeln!(out, "        big: {},", opt_str(get(desc, "long")));
        let _ = writeln!(out, "        sound: {sound},");
        let _ = writeln!(out, "        loud: {},", truthy(get(loc, "loud")));
        let _ = writeln!(out, "        conditions: {bits:#x},");
        out.push_str("        travel: &[\n");
        for r in rules {
            let _ = writeln!(
                out,
                "            TravelOp {{ motion: Motion({}), cond: {}, dest: {}, nodwarves: {} }},",
                r.motion, r.cond, r.dest, r.nodwarves
            );
        }
        out.push_str("        ],\n    },\n");
    }
    out.push_str("];\n");

    // ---- Objects -----------------------------------------------------------
    out.push_str("\nstatic OBJECTS: [Object; NOBJECTS] = [\n");
    for (i, (name, attr)) in objects.iter().enumerate() {
        let immovable = truthy(get(attr, "immovable"));
        let loc_of = |v: &Value| index_of(&locnames, &str_of(v), "location");
        let (plac, fixd) = match get(attr, "locations") {
            None => (0, "Fixed::Free".to_string()),
            Some(Value::Sequence(s)) => {
                (loc_of(&s[0]), format!("Fixed::At(Loc({}))", loc_of(&s[1])))
            }
            Some(v) => (
                loc_of(v),
                if immovable {
                    "Fixed::Immovable"
                } else {
                    "Fixed::Free"
                }
                .to_string(),
            ),
        };
        let _ = writeln!(out, "    Object {{ // {i}: {name}");
        let _ = writeln!(out, "        words: {},", words_literal(get(attr, "words")));
        let _ = writeln!(
            out,
            "        inventory: {},",
            opt_str(get(attr, "inventory"))
        );
        let _ = writeln!(out, "        plac: Loc({plac}),");
        let _ = writeln!(out, "        fixd: {fixd},");
        let _ = writeln!(
            out,
            "        is_treasure: {},",
            truthy(get(attr, "treasure"))
        );
        for field in ["descriptions", "sounds", "texts", "changes"] {
            let _ = writeln!(out, "        {field}: {},", opt_str_list(get(attr, field)));
        }
        out.push_str("    },\n");
    }
    out.push_str("];\n");

    // ---- Obituaries and hints -------------------------------------------------
    out.push_str("\npub(crate) static OBITUARIES: [Obituary; NDEATHS] = [\n");
    for o in obituaries {
        let _ = writeln!(
            out,
            "    Obituary {{ query: {}, yes_response: {} }},",
            opt_str(get(o, "query")),
            opt_str(get(o, "yes_response"))
        );
    }
    out.push_str("];\n");

    out.push_str("\npub(crate) static HINTS: [Hint; NHINTS] = [\n");
    for member in hints {
        let h = &member["hint"];
        let _ = writeln!(
            out,
            "    Hint {{ turns: {}, penalty: {}, question: {}, hint: {} }},",
            int_of(&h["turns"]),
            int_of(&h["penalty"]),
            opt_str(get(h, "question")),
            opt_str(get(h, "hint"))
        );
    }
    out.push_str("];\n");

    // ---- Vocabulary -----------------------------------------------------------
    // Single-letter words of entries marked `oldstyle: false` are ignored
    // in oldstyle mode.
    let mut ignore = String::new();
    let mut collect_ignored = |body: &Value| {
        if get(body, "oldstyle").is_some_and(|v| !truthy(Some(v))) {
            for w in str_list(get(body, "words")) {
                if w.len() == 1 {
                    ignore.push_str(&w.to_uppercase());
                }
            }
        }
    };

    out.push_str("\nstatic MOTIONS: [MotionInfo; NMOTIONS] = [\n");
    for (name, m) in &motions {
        let _ = writeln!(
            out,
            "    MotionInfo {{ words: {} }}, // {name}",
            words_literal(get(m, "words"))
        );
        collect_ignored(m);
    }
    out.push_str("];\n");

    out.push_str("\nstatic ACTIONS: [VerbInfo; NACTIONS] = [\n");
    for (name, a) in &actions {
        let _ = writeln!(
            out,
            "    VerbInfo {{ words: {}, message: {}, noaction: {} }}, // {name}",
            words_literal(get(a, "words")),
            opt_str(get(a, "message")),
            get(a, "noaction").is_some()
        );
        collect_ignored(a);
    }
    out.push_str("];\n");
    let _ = writeln!(out, "\npub(crate) static IGNORE: &str = {ignore:?};");

    // ---- Dwarf starting locations ----------------------------------------------
    let dl: Vec<String> = dwarflocs
        .iter()
        .map(|l| format!("Loc({})", index_of(&locnames, l, "location")))
        .collect();
    let _ = writeln!(
        out,
        "\npub(crate) static DWARFLOCS: [Loc; NDWARVES] = [{}];",
        dl.join(", ")
    );

    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("dungeon.rs");
    fs::write(dest, out).unwrap();
}

/// Compile the YAML travel rules into per-location rule lists.
///
/// This follows make_dungeon.py: each YAML rule expands into one entry
/// per motion verb, all sharing the rule's condition and destination.
/// A rule with no verbs gets the magic motion HERE (1), meaning "fire
/// as soon as the player arrives".
///
/// Conditions map onto the C encoding as follows: none is `Always`;
/// `[pct N]` is `Pct(N)`; `[nodwarves]` is `Pct(100)` plus the
/// `nodwarves` flag (the C code rolls the die even then, so we must
/// too); `[carry OBJ]`, `[with OBJ]` and `[not OBJ STATE]` map directly.
fn buildtravel(
    locs: &[(String, Value)],
    objs: &[(String, Value)],
    motions: &[(String, Value)],
    locnames: &[String],
    msgnames: &[String],
    objnames: &[String],
) -> Vec<Vec<TravelRow>> {
    let mut verbmap: HashMap<String, i64> = HashMap::new();
    for (i, (_, m)) in motions.iter().enumerate() {
        for w in str_list(get(m, "words")) {
            verbmap.insert(w.to_uppercase(), i as i64);
        }
    }

    let dencode = |action: &Value, name: &str| -> String {
        let a = action
            .as_sequence()
            .unwrap_or_else(|| fail(&format!("bad action in {name}")));
        match str_of(&a[0]).as_str() {
            "goto" => {
                let l = index_of(locnames, &str_of(&a[1]), "location in goto clause");
                format!("Dest::Goto(Loc({l}))")
            }
            "special" => format!("Dest::Special({})", int_of(&a[1])),
            "speak" => {
                let m = index_of(msgnames, &str_of(&a[1]), "message in speak clause");
                format!("Dest::Speak(Msg({m}))")
            }
            other => fail(&format!("unknown action {other} in {name}")),
        }
    };

    let cencode = |cond: Option<&Value>, name: &str| -> (String, bool) {
        let Some(cond) = cond else {
            return ("TravelCond::Always".to_string(), false);
        };
        let c = cond
            .as_sequence()
            .unwrap_or_else(|| fail(&format!("bad cond in {name}")));
        let obj = |v: &Value, what: &str| index_of(objnames, &str_of(v), what);
        match str_of(&c[0]).as_str() {
            "nodwarves" => ("TravelCond::Pct(100)".to_string(), true),
            "pct" => {
                let n = int_of(&c[1]);
                assert!((1..100).contains(&n), "pct {n} out of range in {name}");
                (format!("TravelCond::Pct({n})"), false)
            }
            "carry" => (
                format!(
                    "TravelCond::Carry(Obj({}))",
                    obj(&c[1], "object in carry clause")
                ),
                false,
            ),
            "with" => (
                format!(
                    "TravelCond::With(Obj({}))",
                    obj(&c[1], "object in with clause")
                ),
                false,
            ),
            "not" => {
                let o = obj(&c[1], "object in not clause");
                let state = match &c[2] {
                    Value::Number(n) => n.as_i64().unwrap(),
                    s => {
                        let s = str_of(s);
                        let states = str_list(get(&objs[o as usize].1, "states"));
                        states.iter().position(|x| *x == s).unwrap_or_else(|| {
                            fail(&format!(
                                "unmatched state symbol {s} in not clause of {name}"
                            ))
                        }) as i64
                    }
                };
                (format!("TravelCond::Not(Obj({o}), {state})"), false)
            }
            other => fail(&format!("unknown condition {other} in {name}")),
        }
    };

    let mut travel: Vec<Vec<TravelRow>> = Vec::with_capacity(locs.len());
    for (name, loc) in locs {
        let mut rows = Vec::new();
        if let Some(Value::Sequence(rules)) = loc.get("travel") {
            for rule in rules {
                let dest = dencode(&rule["action"], name);
                let (cond, nodwarves) = cencode(get(rule, "cond"), name);
                let mut mots: Vec<i64> = str_list(get(rule, "verbs"))
                    .iter()
                    .map(|v| {
                        *verbmap
                            .get(&v.to_uppercase())
                            .unwrap_or_else(|| fail(&format!("unknown verb {v} in {name}")))
                    })
                    .collect();
                if mots.is_empty() {
                    mots.push(1); // HERE: fire on arrival
                }
                for motion in mots {
                    rows.push(TravelRow {
                        motion,
                        cond: cond.clone(),
                        dest: dest.clone(),
                        on_arrival: motion == 1,
                        nodwarves,
                    });
                }
            }
        }
        travel.push(rows);
    }
    travel
}
