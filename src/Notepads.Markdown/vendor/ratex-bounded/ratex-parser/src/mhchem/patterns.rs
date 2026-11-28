//! Pattern matching (`mhchemParser.patterns`).

use crate::expansion_budget::ExpansionBudget;
use crate::mhchem::data::MhchemData;
use crate::mhchem::error::{MhchemError, MhchemResult};
use fancy_regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

#[derive(Clone, Debug)]
pub enum MatchToken<'a> {
    S(Cow<'a, str>),
    A(Vec<Cow<'a, str>>),
}

pub struct PatternHit<'a> {
    pub token: MatchToken<'a>,
    pub remainder: &'a str,
}

#[derive(Clone, Copy)]
pub enum Beg<'a> {
    Str(&'a str),
    Re(&'a Regex),
}

#[derive(Clone, Copy)]
pub enum End<'a> {
    Str(&'a str),
    Re(&'a Regex),
}

fn prefix_len(input: &str, beg: Beg<'_>) -> Option<usize> {
    match beg {
        Beg::Str(s) => {
            if s.is_empty() {
                Some(0)
            } else {
                input.starts_with(s).then_some(s.len())
            }
        }
        Beg::Re(re) => re
            .find(input)
            .ok()
            .flatten()
            .filter(|m| m.start() == 0)
            .map(|m| m.end()),
    }
}

fn scan_end(input: &str, start: usize, end: End<'_>) -> MhchemResult<Option<(usize, usize)>> {
    let mut i = start;
    let mut braces = 0i32;
    while i < input.len() {
        let rest = &input[i..];
        let m = match end {
            End::Str(s) => {
                if s.is_empty() {
                    None
                } else {
                    rest.starts_with(s).then_some(s.len())
                }
            }
            End::Re(re) => re
                .find(rest)
                .ok()
                .flatten()
                .filter(|m| m.start() == 0)
                .map(|m| m.end()),
        };
        if let Some(len) = m {
            if braces == 0 {
                return Ok(Some((i, i + len)));
            }
        }
        let c = input[i..].chars().next().unwrap();
        if c == '{' {
            braces += 1;
        } else if c == '}' {
            if braces == 0 {
                return Err(MhchemError::ExtraClose);
            }
            braces -= 1;
        }
        i += c.len_utf8();
    }
    Ok(None)
}

/// KaTeX `findObserveGroups`.
#[allow(clippy::too_many_arguments)]
pub fn find_observe_groups<'a>(
    input: &'a str,
    beg_excl: &str,
    beg_incl: Beg<'_>,
    end_incl: &str,
    end_excl: End<'_>,
    part2: Option<(&str, Beg<'_>, &str, End<'_>)>,
    combine: bool,
    resources: &ExpansionBudget,
) -> MhchemResult<Option<PatternHit<'a>>> {
    let ex = if beg_excl.is_empty() {
        0
    } else {
        let Some(n) = prefix_len(input, Beg::Str(beg_excl)) else {
            return Ok(None);
        };
        n
    };
    let rest1 = &input[ex..];
    let open = match prefix_len(rest1, beg_incl) {
        Some(n) => n,
        None => return Ok(None),
    };
    let end_main = if !end_incl.is_empty() {
        End::Str(end_incl)
    } else {
        end_excl
    };
    let e = match scan_end(rest1, open, end_main)? {
        Some(e) => e,
        None => return Ok(None),
    };
    let use_incl = !end_incl.is_empty();
    let cut = if use_incl { e.1 } else { e.0 };
    let m1 = Cow::Borrowed(&rest1[..cut]);
    let after = &rest1[e.1..];

    if let Some((b2e, b2i, e2i, e2e)) = part2 {
        let g2 = find_observe_groups(after, b2e, b2i, e2i, e2e, None, false, resources)?;
        let Some(h2) = g2 else {
            return Ok(None);
        };
        let m2 = match h2.token {
            MatchToken::S(s) => s,
            MatchToken::A(_) => return Err(MhchemError::msg("nested Fog pair")),
        };
        let tok = if combine {
            MatchToken::S(Cow::Owned(resources.format(format_args!("{m1}{m2}"))?))
        } else {
            MatchToken::A(vec![m1, m2])
        };
        return Ok(Some(PatternHit {
            token: tok,
            remainder: h2.remainder,
        }));
    }

    Ok(Some(PatternHit {
        token: MatchToken::S(m1),
        remainder: after,
    }))
}

fn regex_match_token<'a>(re: &Regex, input: &'a str) -> Option<(MatchToken<'a>, usize)> {
    let caps = re.captures(input).ok().flatten()?;
    let m0 = caps.get(0)?;
    let end = m0.end();
    if let Some(g2) = caps.get(2) {
        if !g2.as_str().is_empty() {
            let g1 = caps
                .get(1)
                .map(|g| Cow::Borrowed(g.as_str()))
                .unwrap_or_default();
            return Some((MatchToken::A(vec![g1, Cow::Borrowed(g2.as_str())]), end));
        }
    }
    if let Some(g1) = caps.get(1) {
        return Some((MatchToken::S(Cow::Borrowed(g1.as_str())), end));
    }
    Some((MatchToken::S(Cow::Borrowed(m0.as_str())), end))
}

static RE_AGG_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\([a-z]{1,3}(?=[\),])").unwrap());
static RE_CMD_BRACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\\[a-zA-Z]+\{").unwrap());
static RE_BEFORE_BRACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?=\{)").unwrap());
static RE_NEG_POW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\+\-|\+\/\-|\+|\-|\\pm\s?)?([0-9]+(?:[,.][0-9]+)?|[0-9]*(?:\.[0-9]+)?)\^([+\-]?[0-9]+|\{[+\-]?[0-9]+\})",
    ).unwrap()
});
static RE_SCI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\+\-|\+\/\-|\+|\-|\\pm\s?)?([0-9]+(?:[,.][0-9]+)?|[0-9]*(?:\.[0-9]+))?(\((?:[0-9]+(?:[,.][0-9]+)?|[0-9]*(?:\.[0-9]+))\))?(?:([eE]|\s*(\*|x|\\times|\u{00D7})\s*10\^)([+\-]?[0-9]+|\{[+\-]?[0-9]+\}))?",
    ).unwrap()
});
static RE_SOA_REMAINDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^($|[\s,;\)\]\}])").unwrap());
static RE_SOA_ALT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\((?:\\ca\s?)?\$[amothc]\$\))").unwrap());
static RE_AMOUNT_MAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:(?:(?:\([+\-]?[0-9]+\/[0-9]+\)|[+\-]?(?:[0-9]+|\$[a-z]\$|[a-z])\/[0-9]+|[+\-]?[0-9]+[.,][0-9]+|[+\-]?\.[0-9]+|[+\-]?[0-9]+)(?:[a-z](?=\s*[A-Z]))?)|[+\-]?[a-z](?=\s*[A-Z])|\+(?!\s))",
    ).unwrap()
});
static RE_AMOUNT_DOLLAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\$(?:\(?[+\-]?(?:[0-9]*[a-z]?[+\-])?[0-9]*[a-z](?:[+\-][0-9]*[a-z]?)?\)?|\+|-)\$$",
    )
    .unwrap()
});
static RE_FORMULA_PAREN_ONLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\([a-z]+\)$").unwrap());
static RE_FORMULA_MAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:[a-z]|(?:[0-9\ \+\-\,\.\(\)]+[a-z])+[0-9\ \+\-\,\.\(\)]*|(?:[a-z][0-9\ \+\-\,\.\(\)]+)+[a-z]?)",
    ).unwrap()
});

fn pattern_neg_pow<'a>(
    input: &'a str,
    _resources: &ExpansionBudget,
) -> MhchemResult<Option<PatternHit<'a>>> {
    let Some(c) = RE_NEG_POW.captures(input).ok().flatten() else {
        return Ok(None);
    };
    let full = c.get(0).unwrap();
    let mut v = Vec::new();
    for i in 1..c.len() {
        v.push(
            c.get(i)
                .map(|g| Cow::Borrowed(g.as_str()))
                .unwrap_or_default(),
        );
    }
    Ok(Some(PatternHit {
        token: MatchToken::A(v),
        remainder: &input[full.end()..],
    }))
}

fn pattern_sci<'a>(
    input: &'a str,
    _resources: &ExpansionBudget,
) -> MhchemResult<Option<PatternHit<'a>>> {
    let Some(c) = RE_SCI.captures(input).ok().flatten() else {
        return Ok(None);
    };
    let full = c.get(0).unwrap();
    if full.as_str().is_empty() {
        return Ok(None);
    }
    let mut v = Vec::new();
    for i in 1..c.len() {
        v.push(
            c.get(i)
                .map(|g| Cow::Borrowed(g.as_str()))
                .unwrap_or_default(),
        );
    }
    Ok(Some(PatternHit {
        token: MatchToken::A(v),
        remainder: &input[full.end()..],
    }))
}

fn pattern_state_of_agg<'a>(
    input: &'a str,
    resources: &ExpansionBudget,
) -> MhchemResult<Option<PatternHit<'a>>> {
    if let Some(h) = find_observe_groups(
        input,
        "",
        Beg::Re(&RE_AGG_OPEN),
        ")",
        End::Str(""),
        None,
        false,
        resources,
    )? {
        if RE_SOA_REMAINDER
            .find(h.remainder)
            .ok()
            .flatten()
            .filter(|m| m.start() == 0)
            .is_some()
        {
            return Ok(Some(h));
        }
    }
    let re = &*RE_SOA_ALT;
    if let Some(m) = re.find(input).ok().flatten() {
        return Ok(Some(PatternHit {
            token: MatchToken::S(Cow::Borrowed(m.as_str())),
            remainder: &input[m.end()..],
        }));
    }
    Ok(None)
}

fn pattern_amount<'a>(
    input: &'a str,
    resources: &ExpansionBudget,
) -> MhchemResult<Option<PatternHit<'a>>> {
    if let Some(m) = RE_AMOUNT_MAIN.find(input).ok().flatten() {
        return Ok(Some(PatternHit {
            token: MatchToken::S(Cow::Borrowed(m.as_str())),
            remainder: &input[m.end()..],
        }));
    }
    let Some(h) = find_observe_groups(
        input,
        "",
        Beg::Str("$"),
        "$",
        End::Str(""),
        None,
        false,
        resources,
    )?
    else {
        return Ok(None);
    };
    let MatchToken::S(ref s) = h.token else {
        return Ok(None);
    };
    if RE_AMOUNT_DOLLAR.find(s).ok().flatten().is_some() {
        Ok(Some(PatternHit {
            token: h.token,
            remainder: h.remainder,
        }))
    } else {
        Ok(None)
    }
}

fn pattern_formula<'a>(
    input: &'a str,
    _resources: &ExpansionBudget,
) -> MhchemResult<Option<PatternHit<'a>>> {
    if RE_FORMULA_PAREN_ONLY.is_match(input).unwrap_or(false) {
        return Ok(None);
    }
    Ok(RE_FORMULA_MAIN
        .find(input)
        .ok()
        .flatten()
        .map(|m| PatternHit {
            token: MatchToken::S(Cow::Borrowed(m.as_str())),
            remainder: &input[m.end()..],
        }))
}

pub fn match_pattern<'a>(
    data: &MhchemData,
    pattern_name: &str,
    input: &'a str,
    resources: &ExpansionBudget,
) -> MhchemResult<Option<PatternHit<'a>>> {
    match pattern_name {
        "(-)(9)^(-9)" => pattern_neg_pow(input, resources),
        "(-)(9.,9)(e)(99)" => pattern_sci(input, resources),
        "state of aggregation $" => pattern_state_of_agg(input, resources),
        "amount" | "amount2" => pattern_amount(input, resources),
        "formula$" => pattern_formula(input, resources),
        "^{(...)}" => find_observe_groups(
            input,
            "^{",
            Beg::Str(""),
            "",
            End::Str("}"),
            None,
            false,
            resources,
        ),
        "^($...$)" => find_observe_groups(
            input,
            "^",
            Beg::Str("$"),
            "$",
            End::Str(""),
            None,
            false,
            resources,
        ),
        "^\\x{}{}" => find_observe_groups(
            input,
            "^",
            Beg::Re(&RE_CMD_BRACE),
            "}",
            End::Str(""),
            Some(("", Beg::Str("{"), "}", End::Str(""))),
            true,
            resources,
        ),
        "^\\x{}" => find_observe_groups(
            input,
            "^",
            Beg::Re(&RE_CMD_BRACE),
            "}",
            End::Str(""),
            None,
            false,
            resources,
        ),
        "_{(...)}" => find_observe_groups(
            input,
            "_{",
            Beg::Str(""),
            "",
            End::Str("}"),
            None,
            false,
            resources,
        ),
        "_($...$)" => find_observe_groups(
            input,
            "_",
            Beg::Str("$"),
            "$",
            End::Str(""),
            None,
            false,
            resources,
        ),
        "_\\x{}{}" => find_observe_groups(
            input,
            "_",
            Beg::Re(&RE_CMD_BRACE),
            "}",
            End::Str(""),
            Some(("", Beg::Str("{"), "}", End::Str(""))),
            true,
            resources,
        ),
        "_\\x{}" => find_observe_groups(
            input,
            "_",
            Beg::Re(&RE_CMD_BRACE),
            "}",
            End::Str(""),
            None,
            false,
            resources,
        ),
        "{...}" => find_observe_groups(
            input,
            "",
            Beg::Str("{"),
            "}",
            End::Str(""),
            None,
            false,
            resources,
        ),
        "{(...)}" => find_observe_groups(
            input,
            "{",
            Beg::Str(""),
            "",
            End::Str("}"),
            None,
            false,
            resources,
        ),
        "$...$" => find_observe_groups(
            input,
            "",
            Beg::Str("$"),
            "$",
            End::Str(""),
            None,
            false,
            resources,
        ),
        "${(...)}$" => find_observe_groups(
            input,
            "${",
            Beg::Str(""),
            "",
            End::Str("}$"),
            None,
            false,
            resources,
        ),
        "$(...)$" => find_observe_groups(
            input,
            "$",
            Beg::Str(""),
            "",
            End::Str("$"),
            None,
            false,
            resources,
        ),
        "\\bond{(...)}" => find_observe_groups(
            input,
            "\\bond{",
            Beg::Str(""),
            "",
            End::Str("}"),
            None,
            false,
            resources,
        ),
        "[(...)]" => find_observe_groups(
            input,
            "[",
            Beg::Str(""),
            "",
            End::Str("]"),
            None,
            false,
            resources,
        ),
        "\\x{}{}" => find_observe_groups(
            input,
            "",
            Beg::Re(&RE_CMD_BRACE),
            "}",
            End::Str(""),
            Some(("", Beg::Str("{"), "}", End::Str(""))),
            true,
            resources,
        ),
        "\\x{}" => find_observe_groups(
            input,
            "",
            Beg::Re(&RE_CMD_BRACE),
            "}",
            End::Str(""),
            None,
            false,
            resources,
        ),
        "\\frac{(...)}" => find_observe_groups(
            input,
            "\\frac{",
            Beg::Str(""),
            "",
            End::Str("}"),
            Some(("{", Beg::Str(""), "", End::Str("}"))),
            false,
            resources,
        ),
        "\\overset{(...)}" => find_observe_groups(
            input,
            "\\overset{",
            Beg::Str(""),
            "",
            End::Str("}"),
            Some(("{", Beg::Str(""), "", End::Str("}"))),
            false,
            resources,
        ),
        "\\underset{(...)}" => find_observe_groups(
            input,
            "\\underset{",
            Beg::Str(""),
            "",
            End::Str("}"),
            Some(("{", Beg::Str(""), "", End::Str("}"))),
            false,
            resources,
        ),
        "\\underbrace{(...)}" => find_observe_groups(
            input,
            "\\underbrace{",
            Beg::Str(""),
            "",
            End::Str("}_"),
            Some(("{", Beg::Str(""), "", End::Str("}"))),
            false,
            resources,
        ),
        "\\color{(...)}0" => find_observe_groups(
            input,
            "\\color{",
            Beg::Str(""),
            "",
            End::Str("}"),
            None,
            false,
            resources,
        ),
        "\\color{(...)}{(...)}1" => find_observe_groups(
            input,
            "\\color{",
            Beg::Str(""),
            "",
            End::Str("}"),
            Some(("{", Beg::Str(""), "", End::Str("}"))),
            false,
            resources,
        ),
        "\\color(...){(...)}2" => find_observe_groups(
            input,
            "\\color",
            Beg::Str("\\"),
            "",
            End::Re(&RE_BEFORE_BRACE),
            Some(("{", Beg::Str(""), "", End::Str("}"))),
            false,
            resources,
        ),
        "\\ce{(...)}" => find_observe_groups(
            input,
            "\\ce{",
            Beg::Str(""),
            "",
            End::Str("}"),
            None,
            false,
            resources,
        ),
        _ => {
            let Some(re) = data.regexes.map.get(pattern_name) else {
                return Err(MhchemError::msg(format!(
                    "mhchem bug P: unknown pattern ({pattern_name})"
                )));
            };
            Ok(regex_match_token(re, input).map(|(t, n)| PatternHit {
                token: t,
                remainder: &input[n..],
            }))
        }
    }
}

#[cfg(test)]
mod fog_tests {
    use super::{find_observe_groups, match_pattern, Beg, End, MatchToken};

    #[test]
    fn x_double_brace_second_group_includes_nested_ce_close() {
        use crate::mhchem::data::data;
        let input = r"\underset{\mathrm{red}}{\ce{HgI2}}";
        let hit = match_pattern(
            data(),
            r"\x{}{}",
            input,
            &crate::expansion_budget::ExpansionBudget::default(),
        )
        .unwrap()
        .unwrap();
        let MatchToken::S(s) = hit.token else {
            panic!("expected combined S");
        };
        assert_eq!(s, r"\underset{\mathrm{red}}{\ce{HgI2}}");
        assert!(hit.remainder.is_empty());
    }

    #[test]
    fn underset_splits_nested_mathrm() {
        let input = r"\underset{\mathrm{red}}{\ce{HgI2}}";
        let hit = find_observe_groups(
            input,
            "\\underset{",
            Beg::Str(""),
            "",
            End::Str("}"),
            Some(("{", Beg::Str(""), "", End::Str("}"))),
            false,
            &crate::expansion_budget::ExpansionBudget::default(),
        )
        .unwrap()
        .unwrap();
        let MatchToken::A(parts) = hit.token else {
            panic!("expected pair");
        };
        assert_eq!(parts[0], r"\mathrm{red}");
        assert_eq!(parts[1], r"\ce{HgI2}");
    }
}
