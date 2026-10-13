// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Rendering of message templates.
//!
//! Messages may contain `%d` (an integer), `%s` (a string), `%S` (an
//! "s" unless the previous integer was exactly 1) and `%V` (the
//! program version).

/// Program version, substituted for `%V`.
pub const VERSION: &str = "1.22";

/// An argument for a message template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg<'a> {
    Int(i32),
    Str(&'a str),
}

impl From<i32> for Arg<'_> {
    fn from(n: i32) -> Self {
        Arg::Int(n)
    }
}

impl<'a> From<&'a str> for Arg<'a> {
    fn from(s: &'a str) -> Self {
        Arg::Str(s)
    }
}

/// Fill in a message template.
///
/// When `outdoors` is set, "floor" (as a whole word followed by a space,
/// a full stop or the end of the message) becomes "ground", so objects
/// dropped outside aren't described as lying on the floor.
///
/// Arguments of the wrong kind for their specifier render as `0` or
/// nothing; unknown specifiers are dropped.
pub fn render(template: &str, args: &[Arg], outdoors: bool) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut pluralize = false;
    let mut rest = template;
    while let Some(c) = rest.chars().next() {
        if c == '%' {
            let spec = rest[1..].chars().next();
            match spec {
                Some('d') => {
                    let n = match args.next() {
                        Some(Arg::Int(n)) => *n,
                        _ => 0,
                    };
                    out.push_str(&n.to_string());
                    pluralize = n != 1;
                }
                Some('s') => {
                    if let Some(Arg::Str(s)) = args.next() {
                        out.push_str(s);
                    }
                }
                Some('S') if pluralize => out.push('s'),
                Some('V') => out.push_str(VERSION),
                _ => {}
            }
            rest = &rest[1 + spec.map_or(0, char::len_utf8)..];
        } else if outdoors
            && rest.starts_with("floor")
            && matches!(rest.as_bytes().get(5), None | Some(b' ' | b'.'))
        {
            out.push_str("ground");
            rest = &rest[5..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// Fill in a template with C `printf` semantics for `%s`, `%d` and
/// `%%`.  Used for the handful of messages that echo the player's
/// words back, where no "floor" substitution or pluralizing applies.
pub fn printf(template: &str, args: &[Arg]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('s') => {
                if let Some(Arg::Str(s)) = args.next() {
                    out.push_str(s);
                }
            }
            Some('d') => {
                if let Some(Arg::Int(n)) = args.next() {
                    out.push_str(&n.to_string());
                }
            }
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_integers_and_strings() {
        let s = render(
            "%d of %d for %s",
            &[3.into(), 5.into(), "you".into()],
            false,
        );
        assert_eq!(s, "3 of 5 for you");
    }

    #[test]
    fn pluralizes_on_previous_integer() {
        assert_eq!(render("%d turn%S", &[1.into()], false), "1 turn");
        assert_eq!(render("%d turn%S", &[2.into()], false), "2 turns");
        assert_eq!(render("%d turn%S", &[0.into()], false), "0 turns");
    }

    #[test]
    fn floor_becomes_ground_outdoors_only() {
        let t = "It lies on the floor.";
        assert_eq!(render(t, &[], false), t);
        assert_eq!(render(t, &[], true), "It lies on the ground.");
        assert_eq!(render("the floor", &[], true), "the ground");
        assert_eq!(render("floors here", &[], true), "floors here");
    }

    #[test]
    fn version_and_unknown_specifiers() {
        assert_eq!(render("v%V", &[], false), format!("v{VERSION}"));
        assert_eq!(render("a%qb", &[], false), "ab");
        assert_eq!(render("trailing %", &[], false), "trailing ");
    }

    #[test]
    fn printf_keeps_percent_escapes() {
        assert_eq!(printf("100%% %s", &["sure".into()]), "100% sure");
        assert_eq!(printf("odd %x", &[]), "odd %x");
    }
}
