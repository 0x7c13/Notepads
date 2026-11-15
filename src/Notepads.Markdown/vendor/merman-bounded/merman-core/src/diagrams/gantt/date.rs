use super::*;

pub(super) const JS_MAX_TIME_MILLIS: i64 = 8_640_000_000_000_000;

pub(super) fn js_date_from_unix_millis(milliseconds: i64) -> Option<OffsetDateTime> {
    (-JS_MAX_TIME_MILLIS..=JS_MAX_TIME_MILLIS)
        .contains(&milliseconds)
        .then(|| OffsetDateTime::from_unix_millis(milliseconds, UtcOffset::UTC))
}

#[derive(Debug, Clone)]
enum DayjsFormatItem {
    Literal(String),
    Token(DayjsToken),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DayjsToken {
    Year4,
    Year2,
    Month2,
    Month1,
    MonthNameShort,
    MonthNameLong,
    Day2,
    Day1,
    DayOrdinal,
    Hour24_2,
    Hour24_1,
    Hour12_2,
    Hour12_1,
    Minute2,
    Minute1,
    Second2,
    Second1,
    Millis3,
    Millis2,
    Millis1,
    OffsetColon,
    OffsetNoColon,
    AmPmUpper,
    AmPmLower,
    UnixMs,
    UnixSec,
    WeekdayLong,
    WeekdayShort,
}

fn tokenize_dayjs_format(fmt: &str) -> Vec<DayjsFormatItem> {
    let mut out: Vec<DayjsFormatItem> = Vec::new();

    fn push_lit(out: &mut Vec<DayjsFormatItem>, s: &str) {
        if s.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(DayjsFormatItem::Literal(prev)) => prev.push_str(s),
            _ => out.push(DayjsFormatItem::Literal(s.to_string())),
        }
    }

    let tokens: &[(&str, DayjsToken)] = &[
        ("YYYY", DayjsToken::Year4),
        ("MMMM", DayjsToken::MonthNameLong),
        ("MMM", DayjsToken::MonthNameShort),
        ("MM", DayjsToken::Month2),
        ("M", DayjsToken::Month1),
        ("Do", DayjsToken::DayOrdinal),
        ("DD", DayjsToken::Day2),
        ("D", DayjsToken::Day1),
        ("HH", DayjsToken::Hour24_2),
        ("H", DayjsToken::Hour24_1),
        ("hh", DayjsToken::Hour12_2),
        ("h", DayjsToken::Hour12_1),
        ("mm", DayjsToken::Minute2),
        ("m", DayjsToken::Minute1),
        ("ss", DayjsToken::Second2),
        ("s", DayjsToken::Second1),
        ("SSS", DayjsToken::Millis3),
        ("SS", DayjsToken::Millis2),
        ("S", DayjsToken::Millis1),
        ("ZZ", DayjsToken::OffsetNoColon),
        ("Z", DayjsToken::OffsetColon),
        ("A", DayjsToken::AmPmUpper),
        ("a", DayjsToken::AmPmLower),
        ("x", DayjsToken::UnixMs),
        ("X", DayjsToken::UnixSec),
        ("dddd", DayjsToken::WeekdayLong),
        ("ddd", DayjsToken::WeekdayShort),
        ("YY", DayjsToken::Year2),
    ];

    let bytes = fmt.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'['
            && let Some(end_rel) = fmt[i + 1..].find(']')
        {
            let inside = &fmt[i + 1..i + 1 + end_rel];
            push_lit(&mut out, inside);
            i = i + 1 + end_rel + 1;
            continue;
        }

        let Some(rest) = fmt.get(i..) else {
            break;
        };
        let mut matched: Option<(&str, DayjsToken)> = None;
        for (pat, tok) in tokens {
            if rest.starts_with(pat) {
                matched = Some((*pat, *tok));
                break;
            }
        }
        if let Some((pat, tok)) = matched {
            out.push(DayjsFormatItem::Token(tok));
            i += pat.len();
        } else {
            let Some(ch) = rest.chars().next() else {
                break;
            };
            push_lit(&mut out, &ch.to_string());
            i += ch.len_utf8();
        }
    }

    out
}

#[derive(Debug, Clone, Default)]
struct DayjsParsedParts {
    year: Option<i32>,
    month: Option<u32>,
    day: Option<u32>,
    hour24: Option<u32>,
    hour12: Option<u32>,
    minute: Option<u32>,
    second: Option<u32>,
    millis: Option<u32>,
    ampm_is_pm: Option<bool>,
    offset_minutes: Option<i32>,
    unix_ms: Option<i64>,
}

// Clamps `byte_idx` down to the nearest valid char boundary before splitting,
// so this never panics on multi-byte input; callers already reject a
// shorter-than-requested `head` via their existing length/digit checks.
fn split_at_char_boundary(s: &str, byte_idx: usize) -> (&str, &str) {
    let mut idx = byte_idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    s.split_at(idx)
}

pub(super) fn parse_dayjs_like_strict(date_format: &str, s: &str) -> Option<OffsetDateTime> {
    let fmt = date_format.trim();
    if fmt.is_empty() {
        return None;
    }

    let items = tokenize_dayjs_format(fmt);

    fn parse_signed_i64_prefix(input: &str) -> Option<(i64, &str)> {
        let bytes = input.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        let mut i = 0usize;
        let sign: i64 = if bytes[0] == b'-' {
            i = 1;
            -1
        } else {
            1
        };
        let start_digits = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start_digits {
            return None;
        }
        let v: i64 = input[start_digits..i].parse().ok()?;
        Some((sign.checked_mul(v)?, &input[i..]))
    }

    fn parse_int_exact(s: &str, digits: usize) -> Option<(u32, &str)> {
        if s.len() < digits {
            return None;
        }
        let (head, tail) = split_at_char_boundary(s, digits);
        if head.len() != digits || !head.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let v = head.parse().ok()?;
        Some((v, tail))
    }

    fn parse_int_var(s: &str, min: usize, max: usize) -> Vec<(u32, &str)> {
        let mut out = Vec::new();
        for digits in (min..=max).rev() {
            if let Some((v, tail)) = parse_int_exact(s, digits) {
                out.push((v, tail));
            }
        }
        out
    }

    fn parse_offset(s: &str, with_colon: bool) -> Option<(i32, &str)> {
        let s = s.strip_prefix(|c| c == ' ' || c == '\t').unwrap_or(s);
        if let Some(tail) = s.strip_prefix('Z') {
            return Some((0, tail));
        }
        let (sign, rest) = if let Some(tail) = s.strip_prefix('+') {
            (1i32, tail)
        } else if let Some(tail) = s.strip_prefix('-') {
            (-1i32, tail)
        } else {
            return None;
        };

        let (hh, rest) = parse_int_exact(rest, 2)?;
        let (mm, rest) = if with_colon {
            let rest = rest.strip_prefix(':')?;
            parse_int_exact(rest, 2)?
        } else {
            parse_int_exact(rest, 2)?
        };
        let hh: i32 = hh.try_into().ok()?;
        let mm: i32 = mm.try_into().ok()?;
        if hh > 23 || mm > 59 {
            return None;
        }
        Some((sign * (hh * 60 + mm), rest))
    }

    fn parse_month_name(s: &str) -> Option<(u32, &str)> {
        const MONTHS: [&str; 12] = [
            "january",
            "february",
            "march",
            "april",
            "may",
            "june",
            "july",
            "august",
            "september",
            "october",
            "november",
            "december",
        ];
        const MONTHS_SHORT: [&str; 12] = [
            "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
        ];

        let lower = s.to_lowercase();
        for (i, name) in MONTHS.iter().enumerate() {
            if lower.starts_with(name) {
                let tail = &s[name.len()..];
                return Some(((i as u32) + 1, tail));
            }
        }
        for (i, name) in MONTHS_SHORT.iter().enumerate() {
            if lower.starts_with(name) {
                let tail = &s[name.len()..];
                return Some(((i as u32) + 1, tail));
            }
        }
        None
    }

    fn parse_weekday_name(s: &str) -> Option<&str> {
        const DAYS: [&str; 7] = [
            "sunday",
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
        ];
        const DAYS_SHORT: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

        let lower = s.to_lowercase();
        for name in DAYS {
            if lower.starts_with(name) {
                return Some(&s[name.len()..]);
            }
        }
        for name in DAYS_SHORT {
            if lower.starts_with(name) {
                return Some(&s[name.len()..]);
            }
        }
        None
    }

    fn parse_day_ordinal(s: &str) -> Option<(u32, &str)> {
        let candidates = parse_int_var(s, 1, 2);
        for (day, tail) in candidates {
            let tail_lower = tail.to_lowercase();
            for suffix in ["st", "nd", "rd", "th"] {
                if tail_lower.starts_with(suffix) {
                    return Some((day, &tail[suffix.len()..]));
                }
            }
        }
        None
    }

    fn parse_ampm(s: &str) -> Option<(bool, &str)> {
        let lower = s.to_lowercase();
        if lower.starts_with("am") {
            return Some((false, &s[2..]));
        }
        if lower.starts_with("pm") {
            return Some((true, &s[2..]));
        }
        None
    }

    fn parse_items<'a>(
        items: &[DayjsFormatItem],
        input: &'a str,
        parts: &DayjsParsedParts,
    ) -> Option<(&'a str, DayjsParsedParts)> {
        if items.is_empty() {
            return Some((input, parts.clone()));
        }

        match &items[0] {
            DayjsFormatItem::Literal(lit) => {
                let input = input.strip_prefix(lit.as_str())?;
                parse_items(&items[1..], input, parts)
            }
            DayjsFormatItem::Token(tok) => match tok {
                DayjsToken::Year4 => {
                    let (y, rest) = parse_int_exact(input, 4)?;
                    let mut next = parts.clone();
                    next.year = Some(y as i32);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Year2 => {
                    let (y2, rest) = parse_int_exact(input, 2)?;
                    let y2 = y2 as i32;
                    let year = if y2 <= 68 { 2000 + y2 } else { 1900 + y2 };
                    let mut next = parts.clone();
                    next.year = Some(year);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Month2 => {
                    let (m, rest) = parse_int_exact(input, 2)?;
                    if !(1..=12).contains(&m) {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.month = Some(m);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Month1 => {
                    for (m, rest) in parse_int_var(input, 1, 2) {
                        if !(1..=12).contains(&m) {
                            continue;
                        }
                        let mut next = parts.clone();
                        next.month = Some(m);
                        if let Some(r) = parse_items(&items[1..], rest, &next) {
                            return Some(r);
                        }
                    }
                    None
                }
                DayjsToken::MonthNameShort | DayjsToken::MonthNameLong => {
                    let (m, rest) = parse_month_name(input)?;
                    let mut next = parts.clone();
                    next.month = Some(m);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Day2 => {
                    let (d, rest) = parse_int_exact(input, 2)?;
                    if !(1..=31).contains(&d) {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.day = Some(d);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Day1 => {
                    for (d, rest) in parse_int_var(input, 1, 2) {
                        if !(1..=31).contains(&d) {
                            continue;
                        }
                        let mut next = parts.clone();
                        next.day = Some(d);
                        if let Some(r) = parse_items(&items[1..], rest, &next) {
                            return Some(r);
                        }
                    }
                    None
                }
                DayjsToken::DayOrdinal => {
                    let (d, rest) = parse_day_ordinal(input)?;
                    if !(1..=31).contains(&d) {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.day = Some(d);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Hour24_2 => {
                    let (h, rest) = parse_int_exact(input, 2)?;
                    if h > 23 {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.hour24 = Some(h);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Hour24_1 => {
                    for (h, rest) in parse_int_var(input, 1, 2) {
                        if h > 23 {
                            continue;
                        }
                        let mut next = parts.clone();
                        next.hour24 = Some(h);
                        if let Some(r) = parse_items(&items[1..], rest, &next) {
                            return Some(r);
                        }
                    }
                    None
                }
                DayjsToken::Hour12_2 => {
                    let (h, rest) = parse_int_exact(input, 2)?;
                    if !(1..=12).contains(&h) {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.hour12 = Some(h);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Hour12_1 => {
                    for (h, rest) in parse_int_var(input, 1, 2) {
                        if !(1..=12).contains(&h) {
                            continue;
                        }
                        let mut next = parts.clone();
                        next.hour12 = Some(h);
                        if let Some(r) = parse_items(&items[1..], rest, &next) {
                            return Some(r);
                        }
                    }
                    None
                }
                DayjsToken::Minute2 => {
                    let (m, rest) = parse_int_exact(input, 2)?;
                    if m > 59 {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.minute = Some(m);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Minute1 => {
                    for (m, rest) in parse_int_var(input, 1, 2) {
                        if m > 59 {
                            continue;
                        }
                        let mut next = parts.clone();
                        next.minute = Some(m);
                        if let Some(r) = parse_items(&items[1..], rest, &next) {
                            return Some(r);
                        }
                    }
                    None
                }
                DayjsToken::Second2 => {
                    let (sec, rest) = parse_int_exact(input, 2)?;
                    if sec > 59 {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.second = Some(sec);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Second1 => {
                    for (sec, rest) in parse_int_var(input, 1, 2) {
                        if sec > 59 {
                            continue;
                        }
                        let mut next = parts.clone();
                        next.second = Some(sec);
                        if let Some(r) = parse_items(&items[1..], rest, &next) {
                            return Some(r);
                        }
                    }
                    None
                }
                DayjsToken::Millis3 => {
                    let (ms, rest) = parse_int_exact(input, 3)?;
                    if ms > 999 {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.millis = Some(ms);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Millis2 => {
                    let (ms, rest) = parse_int_exact(input, 2)?;
                    if ms > 99 {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.millis = Some(ms * 10);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::Millis1 => {
                    let (ms, rest) = parse_int_exact(input, 1)?;
                    if ms > 9 {
                        return None;
                    }
                    let mut next = parts.clone();
                    next.millis = Some(ms * 100);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::OffsetColon => {
                    let (mins, rest) = parse_offset(input, true)?;
                    let mut next = parts.clone();
                    next.offset_minutes = Some(mins);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::OffsetNoColon => {
                    let (mins, rest) = parse_offset(input, false)?;
                    let mut next = parts.clone();
                    next.offset_minutes = Some(mins);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::AmPmUpper | DayjsToken::AmPmLower => {
                    let (is_pm, rest) = parse_ampm(input)?;
                    let mut next = parts.clone();
                    next.ampm_is_pm = Some(is_pm);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::UnixMs => {
                    let (ms, rest) = parse_signed_i64_prefix(input)?;
                    let mut next = parts.clone();
                    next.unix_ms = Some(ms);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::UnixSec => {
                    let (sec, rest) = parse_signed_i64_prefix(input)?;
                    let mut next = parts.clone();
                    next.unix_ms = Some(sec.checked_mul(1000)?);
                    parse_items(&items[1..], rest, &next)
                }
                DayjsToken::WeekdayLong | DayjsToken::WeekdayShort => {
                    let rest = parse_weekday_name(input)?;
                    parse_items(&items[1..], rest, parts)
                }
            },
        }
    }

    let parts = DayjsParsedParts::default();
    let (rest, parts) = parse_items(&items, s, &parts)?;
    if !rest.is_empty() {
        return None;
    }

    if let Some(ms) = parts.unix_ms {
        return js_date_from_unix_millis(ms);
    }

    let base_date = crate::runtime::today_local();

    // Dayjs strict parsing defaults missing calendar fields to the *start* of the larger unit:
    // - `YYYY` => Jan 1st
    // - `YYYY-MM` => 1st of the month
    //
    // If the format omits the year entirely (e.g. `MM-DD`), the current year is used.
    let year = parts.year.unwrap_or(base_date.year());
    let month = match (parts.month, parts.year) {
        (Some(m), _) => m,
        (None, Some(_)) => 1,
        (None, None) => base_date.month(),
    };
    let day = match (parts.day, parts.month, parts.year) {
        (Some(d), _, _) => d,
        (None, Some(_), _) => 1,
        (None, None, Some(_)) => 1,
        (None, None, None) => base_date.day(),
    };

    let mut hour = parts.hour24.unwrap_or(0);
    if parts.hour24.is_none()
        && let Some(h12) = parts.hour12
    {
        let mut h = h12 % 12;
        if parts.ampm_is_pm.unwrap_or(false) {
            h += 12;
        }
        hour = h;
    }

    let minute = parts.minute.unwrap_or(0);
    let second = parts.second.unwrap_or(0);
    let millis = parts.millis.unwrap_or(0);

    let local = CivilDate::new(year, month, day)?.at_hms_milli(hour, minute, second, millis)?;

    if let Some(mins) = parts.offset_minutes {
        OffsetDateTime::from_local(local, UtcOffset::from_minutes(mins)?)
    } else {
        local_from_civil(local)
    }
}

pub(super) fn parse_js_date_fallback(s: &str) -> Result<OffsetDateTime> {
    let s = s.trim();

    if let Some(dt) = parse_js_like_ymd_datetime(s).or_else(|| parse_js_like_mdy_hm_datetime(s)) {
        let year = dt.year();
        if !(-10000..=10000).contains(&year) {
            return Err(Error::diagram_parse_fallback(
                "gantt".to_string(),
                format!("Invalid date:{s}"),
            ));
        }
        return Ok(dt);
    }

    if is_ascii_digits(s) {
        let n: i32 = s.parse().map_err(|_| {
            Error::diagram_parse_fallback("gantt".to_string(), format!("Invalid date:{s}"))
        })?;
        let year = if s.len() <= 2 { 2000 + n } else { n };
        if !(-10000..=10000).contains(&year) {
            return Err(Error::diagram_parse_fallback(
                "gantt".to_string(),
                format!("Invalid date:{s}"),
            ));
        }
        let date = CivilDate::new(year, 1, 1).ok_or_else(|| {
            Error::diagram_parse_fallback("gantt".to_string(), format!("Invalid date:{s}"))
        })?;
        return local_from_civil(date.at_midnight()).ok_or_else(|| {
            Error::diagram_parse_fallback("gantt".to_string(), format!("Invalid date:{s}"))
        });
    }

    Err(Error::diagram_parse_fallback(
        "gantt".to_string(),
        format!("Invalid date:{s}"),
    ))
}

fn parse_js_like_ymd_datetime(s: &str) -> Option<OffsetDateTime> {
    fn parse_u32(s: &str) -> Option<u32> {
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        s.parse().ok()
    }

    fn split_once(s: &str, ch: char) -> Option<(&str, &str)> {
        let idx = s.find(ch)?;
        Some((&s[..idx], &s[idx + 1..]))
    }

    fn parse_timezone_offset_minutes(s: &str) -> Option<(i32, &str)> {
        let s = s.trim_start();
        if let Some(rest) = s.strip_prefix(['Z', 'z']) {
            return Some((0, rest));
        }
        let (sign, rest) = if let Some(rest) = s.strip_prefix('+') {
            (1i32, rest)
        } else if let Some(rest) = s.strip_prefix('-') {
            (-1i32, rest)
        } else {
            return None;
        };

        let (hh_str, rest) = split_at_char_boundary(rest, 2);
        let hh = parse_u32(hh_str)? as i32;

        let (mm, rest) = if let Some(rest) = rest.strip_prefix(':') {
            let (mm_str, rest) = split_at_char_boundary(rest, 2);
            (parse_u32(mm_str)? as i32, rest)
        } else {
            let (mm_str, rest) = split_at_char_boundary(rest, 2);
            (parse_u32(mm_str)? as i32, rest)
        };

        if hh > 23 || mm > 59 {
            return None;
        }
        Some((sign * (hh * 60 + mm), rest))
    }

    fn js_date_only_is_iso_utc(year_str: &str, month_str: &str, day_str: &str) -> bool {
        // JavaScript treats date-only ISO 8601 strings (`YYYY-MM-DD`) as UTC. Non-ISO variants
        // such as `2019-09-1` (non-zero-padded day) are interpreted as local time in V8.
        let year_digits = year_str.strip_prefix(['+', '-']).unwrap_or(year_str);
        let year_is_iso = if year_str.starts_with(['+', '-']) {
            year_digits.len() == 6
        } else {
            year_digits.len() == 4
        };
        year_is_iso && month_str.len() == 2 && day_str.len() == 2
    }

    let (date_part, mut rest) = {
        let mut end = s.len();
        for (i, c) in s.char_indices() {
            if c == 'T' || c == 't' || c.is_whitespace() {
                end = i;
                break;
            }
        }
        (&s[..end], &s[end..])
    };

    let sep = if date_part.contains('-') {
        '-'
    } else if date_part.contains('/') {
        '/'
    } else {
        return None;
    };

    let (year_month, day_str) = date_part.rsplit_once(sep)?;
    let (year_str, month_str) = year_month.rsplit_once(sep)?;
    let year_digits = year_str.strip_prefix(['+', '-']).unwrap_or(year_str);
    let signed_year = year_str.starts_with(['+', '-']);
    let valid_year_shape = if signed_year {
        sep == '-' && year_digits.len() == 6
    } else {
        (1..=4).contains(&year_digits.len())
    };
    if !valid_year_shape {
        return None;
    }
    if !year_digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // ECMAScript reserves the expanded-year spelling `-000000` as invalid. Do not collapse
    // signed zero into civil year zero or emulate implementation-defined legacy date parsing.
    if year_str == "-000000" {
        return None;
    }
    let year: i32 = year_str.parse().ok()?;
    let month = parse_u32(month_str)?;
    let day = parse_u32(day_str)?;
    let date = CivilDate::new(year, month, day)?;

    let mut second: u32 = 0;
    let mut millis: u32 = 0;
    let mut tz_minutes: Option<i32> = None;

    rest = rest.trim_start();
    if rest.is_empty() {
        let local = date.at_midnight();
        if sep == '-' && js_date_only_is_iso_utc(year_str, month_str, day_str) {
            return OffsetDateTime::from_local(local, UtcOffset::UTC);
        }
        return local_from_civil(local);
    }

    if let Some(r) = rest.strip_prefix(['T', 't']) {
        rest = r;
    }
    rest = rest.trim_start();

    let (hh_str, rest2) = split_once(rest, ':')?;
    let hour = parse_u32(hh_str)?;
    let (mm_str, mut rest3) = {
        let (mm_str, rest) = split_at_char_boundary(rest2, 2);
        (mm_str, rest)
    };
    let minute = parse_u32(mm_str)?;

    if let Some(r) = rest3.strip_prefix(':') {
        let (ss_str, mut rest4) = {
            let (ss_str, rest) = split_at_char_boundary(r, 2);
            (ss_str, rest)
        };
        second = parse_u32(ss_str)?;

        if let Some(r) = rest4.strip_prefix('.') {
            let fraction_len = r.bytes().take_while(u8::is_ascii_digit).count();
            let notepads_markdown_digits: String = r.chars().take(fraction_len.min(3)).collect();
            if notepads_markdown_digits.is_empty() {
                return None;
            }
            millis = match notepads_markdown_digits.len() {
                1 => parse_u32(&notepads_markdown_digits)? * 100,
                2 => parse_u32(&notepads_markdown_digits)? * 10,
                _ => parse_u32(&notepads_markdown_digits)?,
            };
            rest4 = &r[fraction_len..];
        }

        rest3 = rest4;
    }

    rest3 = rest3.trim_start();
    if !rest3.is_empty() {
        if let Some((mins, tail)) = parse_timezone_offset_minutes(rest3) {
            tz_minutes = Some(mins);
            if !tail.trim().is_empty() {
                return None;
            }
        } else {
            return None;
        }
    }

    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let local = date.at_hms_milli(hour, minute, second, millis)?;

    if let Some(mins) = tz_minutes {
        return OffsetDateTime::from_local(local, UtcOffset::from_minutes(mins)?);
    }

    local_from_civil(local)
}

fn parse_js_like_mdy_hm_datetime(s: &str) -> Option<OffsetDateTime> {
    // V8 parses strings like `08-08-09-01:00` as local time using an `MM-DD-YY-HH:mm` heuristic.
    // Mermaid gantt falls back to `new Date(str)` when dayjs strict parsing fails, so we mirror
    // that behavior for parity (see `repo-ref/mermaid/packages/mermaid/src/diagrams/gantt/ganttDb.js`).
    //
    // Notes:
    // - Two-digit years follow JS Date semantics: 00–49 => 2000–2049, 50–99 => 1950–1999.
    // - No timezone/offset is present; interpret as local time.
    let s = s.trim();
    let mut parts = s.splitn(4, '-');
    let month_str = parts.next()?;
    let day_str = parts.next()?;
    let year_str = parts.next()?;
    let time_str = parts.next()?;

    fn parse_u32(s: &str) -> Option<u32> {
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        s.parse().ok()
    }

    let month = parse_u32(month_str)?;
    let day = parse_u32(day_str)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let year_raw: i32 = year_str.parse().ok()?;
    let year = if year_str.len() == 2 {
        if (0..=49).contains(&year_raw) {
            2000 + year_raw
        } else if (50..=99).contains(&year_raw) {
            1900 + year_raw
        } else {
            return None;
        }
    } else {
        year_raw
    };

    let (hour_str, rest) = time_str.split_once(':')?;
    let minute_str = rest.get(..2).unwrap_or(rest);
    let rest = rest.get(2..).unwrap_or("");
    let minute = parse_u32(minute_str)?;
    let hour = parse_u32(hour_str)?;
    if hour > 23 || minute > 59 {
        return None;
    }

    let (second, millis) = if let Some(rest) = rest.strip_prefix(':') {
        let second_str = rest.get(..2).unwrap_or(rest);
        let rest = rest.get(2..).unwrap_or("");
        let second = parse_u32(second_str)?;
        if second > 59 {
            return None;
        }
        let millis = if let Some(rest) = rest.strip_prefix('.') {
            let notepads_markdown_str = rest
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>();
            if notepads_markdown_str.is_empty() {
                0
            } else {
                let ms = parse_u32(&notepads_markdown_str).unwrap_or(0);
                ms.min(999)
            }
        } else {
            0
        };
        (second, millis)
    } else {
        (0u32, 0u32)
    };

    let local = CivilDate::new(year, month, day)?.at_hms_milli(hour, minute, second, millis)?;
    local_from_civil(local)
}

fn is_ascii_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_js_regex_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

fn is_gantt_ref_id_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b' '
}

fn gantt_ref_id_prefix_len(s: &str) -> usize {
    s.bytes().take_while(|b| is_gantt_ref_id_byte(*b)).count()
}

pub(super) fn relative_ref_ids_range(s: &str, keyword: &str) -> Option<std::ops::Range<usize>> {
    let rest = s.strip_prefix(keyword)?;
    let rest_start = keyword.len();

    let ws_len: usize = rest
        .chars()
        .take_while(|c| is_js_regex_whitespace(*c))
        .map(char::len_utf8)
        .sum();
    if ws_len == 0 {
        return None;
    }

    let mut start = ws_len;
    loop {
        let refs = &rest[start..];
        let id_len = gantt_ref_id_prefix_len(refs);
        if id_len > 0 {
            let start = rest_start + start;
            return Some(start..start + id_len);
        }

        let (previous_start, _) = rest[..start].char_indices().next_back()?;
        if previous_start == 0 {
            return None;
        }
        start = previous_start;
    }
}

pub(super) fn relative_ref_ids<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
    let range = relative_ref_ids_range(s, keyword)?;
    Some(&s[range])
}

pub(super) fn get_start_date(
    db: &GanttDb,
    date_format: &str,
    raw: &str,
) -> Result<Option<OffsetDateTime>> {
    let s = raw.trim();

    if let Some(ids) = relative_ref_ids(s, "after") {
        let mut latest: Option<Option<OffsetDateTime>> = None;
        for id in ids.split(' ') {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            let Some(task) = db.find_task_by_id(id) else {
                continue;
            };
            if latest.is_none() {
                latest = Some(task.end_time);
                continue;
            }
            let Some(current_best) = latest else {
                continue;
            };
            let (Some(task_end), Some(best_end)) = (task.end_time, current_best) else {
                continue;
            };
            if task_end > best_end {
                latest = Some(Some(task_end));
            }
        }
        return Ok(match latest {
            Some(end) => end,
            None => today_midnight_local(),
        });
    }

    // Mermaid's ganttDb special-cases timestamp formats `x` / `X`: for positive integer strings,
    // it uses `new Date(Number(str))` rather than strict dayjs parsing. This treats the numeric
    // payload as *milliseconds* for both `x` and `X`.
    let fmt = date_format.trim();
    if (fmt == "x" || fmt == "X")
        && is_ascii_digits(s)
        && let Ok(ms) = s.parse::<i64>()
        && let Some(dt) = js_date_from_unix_millis(ms)
    {
        return Ok(Some(dt));
    }

    if let Some(dt) = parse_dayjs_like_strict(date_format, s) {
        return Ok(Some(dt));
    }

    let dt = parse_js_date_fallback(s)?;
    let year = dt.year();
    if !(-10000..=10000).contains(&year) {
        return Err(Error::diagram_parse_fallback(
            "gantt".to_string(),
            format!("Invalid date:{s}"),
        ));
    }
    Ok(Some(dt))
}

pub(super) fn parse_duration(str_: &str) -> (f64, String) {
    let s = str_.trim();
    let bytes = s.as_bytes();

    let mut end = 0usize;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == 0 {
        return (f64::NAN, "ms".to_string());
    }

    if bytes.get(end) == Some(&b'.') {
        end += 1;
        let fraction_start = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == fraction_start {
            return (f64::NAN, "ms".to_string());
        }
    }

    let value = &s[..end];
    let unit = &s[end..];
    if !matches!(unit, "M" | "d" | "h" | "m" | "s" | "w" | "y" | "ms") {
        return (f64::NAN, "ms".to_string());
    }

    (value.parse().unwrap_or(f64::NAN), unit.to_string())
}

fn trunc_f64_to_i64(value: f64) -> Option<i64> {
    const I64_MIN_AS_F64: f64 = -9_223_372_036_854_775_808.0;
    const I64_MAX_EXCLUSIVE_AS_F64: f64 = 9_223_372_036_854_775_808.0;

    if !value.is_finite() {
        return None;
    }
    let truncated = value.trunc();
    if !(I64_MIN_AS_F64..I64_MAX_EXCLUSIVE_AS_F64).contains(&truncated) {
        return None;
    }
    Some(truncated as i64)
}

fn add_milliseconds(dt: OffsetDateTime, value: f64, scale: f64) -> Option<OffsetDateTime> {
    let milliseconds = trunc_f64_to_i64(value * scale)?;
    dt.checked_add_millis(milliseconds)
}

fn add_duration(dt: OffsetDateTime, value: f64, unit: &str) -> Option<OffsetDateTime> {
    if !value.is_finite() {
        return None;
    }
    match unit {
        "ms" => add_milliseconds(dt, value, 1.0),
        "s" => add_milliseconds(dt, value, 1_000.0),
        "m" => add_milliseconds(dt, value, 60_000.0),
        "h" => add_milliseconds(dt, value, 3_600_000.0),
        "d" => {
            if value.fract() == 0.0 {
                add_days_local(dt, trunc_f64_to_i64(value)?)
            } else {
                add_milliseconds(dt, value, 86_400_000.0)
            }
        }
        "w" => {
            if value.fract() == 0.0 {
                let days = trunc_f64_to_i64(value)?.checked_mul(7)?;
                add_days_local(dt, days)
            } else {
                add_milliseconds(dt, value, 604_800_000.0)
            }
        }
        "M" => {
            if value.fract() == 0.0 {
                add_months_local(dt, trunc_f64_to_i64(value)?)
            } else {
                None
            }
        }
        "y" => {
            if value.fract() == 0.0 {
                add_years_local(dt, trunc_f64_to_i64(value)?)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn get_end_date(
    db: &GanttDb,
    prev_time: OffsetDateTime,
    date_format: &str,
    raw: &str,
    inclusive: bool,
) -> Result<Option<OffsetDateTime>> {
    let s = raw.trim();

    if let Some(ids) = relative_ref_ids(s, "until") {
        let mut earliest: Option<Option<OffsetDateTime>> = None;
        for id in ids.split(' ') {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            let Some(task) = db.find_task_by_id(id) else {
                continue;
            };
            if earliest.is_none() {
                earliest = Some(task.start_time);
                continue;
            }
            let Some(current_best) = earliest else {
                continue;
            };
            let (Some(task_start), Some(best_start)) = (task.start_time, current_best) else {
                continue;
            };
            if task_start < best_start {
                earliest = Some(Some(task_start));
            }
        }
        return Ok(match earliest {
            Some(start) => start,
            None => today_midnight_local(),
        });
    }

    if let Some(mut dt) = parse_dayjs_like_strict(date_format, s) {
        if inclusive {
            dt = add_days_local(dt, 1).unwrap_or(dt);
        }
        return Ok(Some(dt));
    }

    let (value, unit) = parse_duration(s);
    if value.is_finite()
        && let Some(new_dt) = add_duration(prev_time, value, &unit)
    {
        return Ok(Some(new_dt));
    }

    Ok(Some(prev_time))
}

pub(super) fn is_strict_yyyy_mm_dd(s: &str) -> bool {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() != 10
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || bytes[4] != b'-'
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || bytes[7] != b'-'
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    s.parse::<CivilDate>().is_ok()
}

pub(super) fn month_short_name(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "",
    }
}

pub(super) fn month_long_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

pub(super) fn ordinal_suffix(n: u32) -> &'static str {
    let n_mod_100 = n % 100;
    if (11..=13).contains(&n_mod_100) {
        return "th";
    }
    match n % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

pub(super) fn format_dayjs_like(dt: OffsetDateTime, fmt: &str) -> String {
    let fmt = fmt.trim();
    if fmt.is_empty() {
        return String::new();
    }

    let items = tokenize_dayjs_format(fmt);
    let local = crate::runtime::datetime_to_local(dt);
    let civil = local.local_datetime();

    let mut out = String::new();
    for item in items {
        match item {
            DayjsFormatItem::Literal(s) => out.push_str(&s),
            DayjsFormatItem::Token(tok) => match tok {
                DayjsToken::Year4 => out.push_str(&format!("{:04}", civil.year())),
                DayjsToken::Year2 => {
                    out.push_str(&format!("{:02}", (civil.year().rem_euclid(100))))
                }
                DayjsToken::Month2 => out.push_str(&format!("{:02}", civil.month())),
                DayjsToken::Month1 => out.push_str(&format!("{}", civil.month())),
                DayjsToken::MonthNameShort => out.push_str(month_short_name(civil.month())),
                DayjsToken::MonthNameLong => out.push_str(month_long_name(civil.month())),
                DayjsToken::Day2 => out.push_str(&format!("{:02}", civil.day())),
                DayjsToken::Day1 => out.push_str(&format!("{}", civil.day())),
                DayjsToken::DayOrdinal => {
                    let d = civil.day();
                    out.push_str(&format!("{d}{}", ordinal_suffix(d)));
                }
                DayjsToken::Hour24_2 => out.push_str(&format!("{:02}", civil.hour())),
                DayjsToken::Hour24_1 => out.push_str(&format!("{}", civil.hour())),
                DayjsToken::Hour12_2 => {
                    let mut h = civil.hour() % 12;
                    if h == 0 {
                        h = 12;
                    }
                    out.push_str(&format!("{:02}", h));
                }
                DayjsToken::Hour12_1 => {
                    let mut h = civil.hour() % 12;
                    if h == 0 {
                        h = 12;
                    }
                    out.push_str(&format!("{}", h));
                }
                DayjsToken::Minute2 => out.push_str(&format!("{:02}", civil.minute())),
                DayjsToken::Minute1 => out.push_str(&format!("{}", civil.minute())),
                DayjsToken::Second2 => out.push_str(&format!("{:02}", civil.second())),
                DayjsToken::Second1 => out.push_str(&format!("{}", civil.second())),
                DayjsToken::Millis3 => out.push_str(&format!("{:03}", civil.millisecond())),
                DayjsToken::Millis2 => out.push_str(&format!("{:02}", civil.millisecond() / 10)),
                DayjsToken::Millis1 => out.push_str(&format!("{}", civil.millisecond() / 100)),
                DayjsToken::OffsetColon | DayjsToken::OffsetNoColon => {
                    let secs = local.offset().seconds();
                    let sign = if secs < 0 { '-' } else { '+' };
                    let secs = secs.abs();
                    let hh = secs / 3600;
                    let mm = (secs % 3600) / 60;
                    match tok {
                        DayjsToken::OffsetColon => out.push_str(&format!("{sign}{hh:02}:{mm:02}")),
                        DayjsToken::OffsetNoColon => out.push_str(&format!("{sign}{hh:02}{mm:02}")),
                        _ => {}
                    }
                }
                DayjsToken::AmPmUpper | DayjsToken::AmPmLower => {
                    let is_pm = civil.hour() >= 12;
                    let s = if is_pm { "PM" } else { "AM" };
                    match tok {
                        DayjsToken::AmPmUpper => out.push_str(s),
                        DayjsToken::AmPmLower => out.push_str(&s.to_lowercase()),
                        _ => {}
                    }
                }
                DayjsToken::UnixMs => out.push_str(&dt.timestamp_millis().to_string()),
                DayjsToken::UnixSec => out.push_str(&dt.timestamp_seconds().to_string()),
                DayjsToken::WeekdayLong => out.push_str(civil.weekday().full_name()),
                DayjsToken::WeekdayShort => out.push_str(civil.weekday().short_name()),
            },
        }
    }
    out
}
