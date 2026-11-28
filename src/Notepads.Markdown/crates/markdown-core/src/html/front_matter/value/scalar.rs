// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Compatibility with yaml 2.9.0's core scalar schema and explicitly named 1.1 tags.
//! The schema rules and timestamp conversion follow yaml (ISC), Copyright Eemeli Aro
//! <eemeli@gmail.com>; see third-party-notices for its full permission notice.
//! Upstream: https://github.com/eemeli/yaml/tree/v2.9.0/src/schema

use super::Value;
use saphyr_parser::ScalarStyle;

pub(super) fn resolve(
    source: &str,
    style: ScalarStyle,
    tag: Option<&str>,
    yaml11: bool,
) -> Result<Value, String> {
    let plain = style == ScalarStyle::Plain;
    let requested = tag.and_then(|s| s.strip_prefix("tag:yaml.org,2002:"));
    if requested == Some("str") || tag.is_some() && requested.is_none() {
        return Ok(Value::Text(source.to_owned()));
    }
    if requested == Some("timestamp") {
        return timestamp(source)
            .map(Value::Date)
            .ok_or_else(|| "!!timestamp expects a date, starting with yyyy-mm-dd".to_owned());
    }
    if requested == Some("binary") {
        return Ok(Value::Binary(binary(source)));
    }
    if !plain && requested.is_none() {
        return Ok(Value::Text(source.to_owned()));
    }
    let allowed = |name| requested.is_none() || requested == Some(name);
    if allowed("null") && matches!(source, "" | "~" | "null" | "Null" | "NULL") {
        return Ok(Value::Null);
    }
    if allowed("bool") {
        if matches!(source, "true" | "True" | "TRUE")
            || yaml11
                && matches!(
                    source,
                    "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON"
                )
        {
            return Ok(Value::Bool(true));
        }
        if matches!(source, "false" | "False" | "FALSE")
            || yaml11
                && matches!(
                    source,
                    "n" | "N" | "no" | "No" | "NO" | "off" | "Off" | "OFF"
                )
        {
            return Ok(Value::Bool(false));
        }
    }
    if allowed("int")
        && let Some(n) = integer(source, yaml11)
    {
        return Ok(Value::Number(n));
    }
    if yaml11 && let Some(n) = sexagesimal(source) {
        let integral = !source.contains('.');
        if integral && allowed("int") || !integral && allowed("float") {
            return Ok(Value::Number(n));
        }
    }
    if allowed("float")
        && let Some(n) = float(source, yaml11)
    {
        return Ok(Value::Number(n));
    }
    if yaml11
        && requested.is_none()
        && let Some(date) = timestamp(source)
    {
        return Ok(Value::Date(date));
    }
    Ok(Value::Text(source.to_owned()))
}

fn integer(source: &str, yaml11: bool) -> Option<f64> {
    let clean;
    let text = if yaml11 {
        clean = source.replace('_', "");
        clean.as_str()
    } else {
        source
    };
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    let negative = text.starts_with('-');
    let radix = if yaml11 {
        if digits.starts_with("0b") {
            2
        } else if digits.starts_with("0x") {
            16
        } else if digits.len() > 1 && digits.starts_with('0') {
            8
        } else {
            10
        }
    } else if text.starts_with("0o") {
        8
    } else if text.starts_with("0x") {
        16
    } else {
        10
    };
    if !yaml11 && radix != 10 && (negative || text.starts_with('+')) {
        return None;
    }
    let number = if radix == 10 {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        text.parse::<f64>().ok()?
    } else {
        let skip = if yaml11 && radix == 8 { 1 } else { 2 };
        let digits = digits.get(skip..)?;
        if digits.is_empty() || !digits.chars().all(|c| c.to_digit(radix).is_some()) {
            return None;
        }
        // Power-of-two radices are rounded once, like JS parseInt, including ties-to-even.
        let bits_per_digit = radix.trailing_zeros() as usize;
        let mut bits = Vec::with_capacity(digits.len() * bits_per_digit);
        for c in digits.chars() {
            let n = c.to_digit(radix)?;
            for shift in (0..bits_per_digit).rev() {
                bits.push((n >> shift) & 1);
            }
        }
        let start = bits.iter().position(|&b| b != 0).unwrap_or(bits.len());
        let bits = &bits[start..];
        let used = bits.len().min(53);
        let mut mantissa = bits[..used].iter().fold(0u64, |n, b| n * 2 + u64::from(*b));
        if bits.len() > used
            && bits[used] != 0
            && (bits[used + 1..].iter().any(|&b| b != 0) || mantissa & 1 != 0)
        {
            mantissa += 1;
        }
        let exponent = bits.len().saturating_sub(used);
        let n = if exponent > 1024 {
            f64::INFINITY
        } else {
            mantissa as f64 * 2f64.powi(exponent as i32)
        };
        if negative { -n } else { n }
    };
    Some(number)
}

fn float(source: &str, yaml11: bool) -> Option<f64> {
    let n = match source {
        ".nan" | ".NaN" | ".NAN" => Some(f64::NAN),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => Some(f64::NEG_INFINITY),
        _ => None,
    };
    if n.is_some() {
        return n;
    }
    let clean;
    let text = if yaml11 {
        clean = source.replace('_', "");
        clean.as_str()
    } else {
        source
    };
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (base, exp) = digits
        .split_once(['e', 'E'])
        .map_or((digits, None), |(a, b)| (a, Some(b)));
    if let Some(exp) = exp {
        let exp = exp.strip_prefix(['-', '+']).unwrap_or(exp);
        if exp.is_empty() || !exp.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    let mut dot = false;
    let mut count = 0usize;
    for b in base.bytes() {
        if b == b'.' && !dot {
            dot = true;
        } else if b.is_ascii_digit() {
            count += 1;
        } else {
            return None;
        }
    }
    if count == 0 || !dot && exp.is_none() {
        return None;
    }
    text.parse().ok()
}

fn sexagesimal(source: &str) -> Option<f64> {
    let text = source.strip_prefix(['-', '+']).unwrap_or(source);
    let mut parts = text.split(':').peekable();
    let first = parts.next()?;
    if parts.peek().is_none()
        || first.is_empty()
        || !first.as_bytes()[0].is_ascii_digit()
        || !first.bytes().all(|b| b.is_ascii_digit() || b == b'_')
    {
        return None;
    }
    let mut n = first.replace('_', "").parse::<f64>().ok()?;
    while let Some(part) = parts.next() {
        let (whole, fraction) = part
            .split_once('.')
            .map_or((part, None), |(a, b)| (a, Some(b)));
        if whole.is_empty() || whole.len() > 2 || !whole.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let whole = whole.parse::<u32>().ok()?;
        if whole >= 60 {
            return None;
        }
        let mut piece = whole as f64;
        if let Some(fraction) = fraction {
            if parts.peek().is_some() || !fraction.bytes().all(|b| b.is_ascii_digit() || b == b'_')
            {
                return None;
            }
            piece += format!("0.{}", fraction.replace('_', ""))
                .parse::<f64>()
                .ok()?;
        }
        n = n * 60.0 + piece;
    }
    Some(if source.starts_with('-') { -n } else { n })
}

pub(super) fn number(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_owned()
    } else if n == f64::INFINITY {
        "Infinity".to_owned()
    } else if n == f64::NEG_INFINITY {
        "-Infinity".to_owned()
    } else if n == 0.0 {
        "0".to_owned()
    } else {
        ryu_js::Buffer::new().format(n).to_owned()
    }
}

/// A scalar inside a collection-valued property key is stringified from its
/// YAML AST, rather than the JS value. yaml 2.9.0 preserves numeric format and
/// decimal precision here, while still canonicalizing radix digits/exponents.
/// Keep this separate from `number`: ordinary JS property coercion and rendered
/// scalar values intentionally discard this source formatting.
pub(super) fn key_number(n: f64, source: &str, tag: Option<&str>, yaml11: bool) -> String {
    if !n.is_finite() {
        return if n.is_nan() {
            ".nan"
        } else if n < 0.0 {
            "-.inf"
        } else {
            ".inf"
        }
        .to_owned();
    }
    let digits = source.strip_prefix(['-', '+']).unwrap_or(source);
    let radix = if digits.starts_with("0x") {
        Some((16, "0x"))
    } else if !yaml11 && source.starts_with("0o") {
        Some((8, "0o"))
    } else if yaml11 && digits.starts_with("0b") {
        Some((2, "0b"))
    } else if yaml11
        && digits.len() > 1
        && digits.starts_with('0')
        && !digits.contains(['.', 'e', 'E', ':'])
    {
        Some((8, "0"))
    } else {
        None
    };
    if let Some((radix, prefix)) = radix {
        return format!(
            "{}{prefix}{}",
            if n < 0.0 { "-" } else { "" },
            integer_radix(n.abs(), radix)
        );
    }
    if source.contains(['e', 'E']) {
        return exponential(n);
    }
    let mut output = if n == 0.0 && n.is_sign_negative() {
        "-0".to_owned()
    } else {
        number(n)
    };
    if !output.contains('e')
        && matches!(tag, None | Some("tag:yaml.org,2002:float"))
        && let Some((_, fraction)) = source.split_once('.')
        && fraction.bytes().rev().find(|b| !yaml11 || *b != b'_') == Some(b'0')
    {
        let precision = if yaml11 {
            fraction.bytes().filter(|b| *b != b'_').count()
        } else {
            fraction.len()
        };
        let actual = output
            .split_once('.')
            .map_or(0, |(_, fraction)| fraction.len());
        if precision > actual {
            if !output.contains('.') {
                output.push('.');
            }
            output.extend(std::iter::repeat_n('0', precision - actual));
        }
    }
    output
}

// JS Number.toString(radix) for the supported power-of-two integer radices.
// Expanding the already rounded IEEE-754 value also handles values above u64.
fn integer_radix(n: f64, radix: u32) -> String {
    if n == 0.0 {
        return "0".to_owned();
    }
    let bits = n.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    let mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let binary = if exponent <= 52 {
        format!("{:b}", mantissa >> (52 - exponent))
    } else {
        format!("{mantissa:b}{}", "0".repeat((exponent - 52) as usize))
    };
    let width = radix.trailing_zeros() as usize;
    let mut result = Vec::with_capacity(binary.len().div_ceil(width));
    let mut end = binary.len();
    while end > 0 {
        let start = end.saturating_sub(width);
        let digit = binary.as_bytes()[start..end]
            .iter()
            .fold(0u8, |n, b| n * 2 + (b - b'0'));
        result.push(b"0123456789abcdef"[digit as usize] as char);
        end = start;
    }
    result.into_iter().rev().collect()
}

// JS Number.toExponential() uses the shortest round-tripping significand, as
// does ryu-js. Convert its fixed/scientific spelling without rounding again.
fn exponential(n: f64) -> String {
    if n == 0.0 {
        return "0e+0".to_owned();
    }
    let shortest = number(n.abs());
    let (coefficient, exponent) = if let Some((coefficient, exponent)) = shortest.split_once('e') {
        (
            coefficient.to_owned(),
            exponent.parse::<i32>().expect("ryu-js exponent"),
        )
    } else {
        let decimal = shortest.find('.').unwrap_or(shortest.len());
        let mut digits = shortest.replace('.', "");
        let first = digits.find(|c| c != '0').expect("nonzero finite number");
        let exponent = decimal as i32 - first as i32 - 1;
        digits = digits[first..].trim_end_matches('0').to_owned();
        if digits.len() > 1 {
            digits.insert(1, '.');
        }
        (digits, exponent)
    };
    format!(
        "{}{coefficient}e{exponent:+}",
        if n < 0.0 { "-" } else { "" }
    )
}

// Node Buffer.from(base64) ignores nonalphabet characters, accepts URL-safe digits,
// stops at padding, and silently discards an incomplete final sextet.
fn binary(source: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(source.len() * 3 / 4);
    let mut bits = 0u32;
    let mut count = 0u32;
    for b in source.bytes() {
        let digit = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        bits = (bits << 6) | u32::from(digit);
        count += 6;
        if count >= 8 {
            count -= 8;
            result.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    result
}

fn timestamp(source: &str) -> Option<String> {
    let (date, time) = source
        .find(['t', 'T', ' ', '\t'])
        .map_or((source, None), |i| {
            (
                &source[..i],
                Some(source[i + 1..].trim_start_matches([' ', '\t'])),
            )
        });
    let mut parts = date.split('-');
    let ys = parts.next()?;
    let ms = parts.next()?;
    let ds = parts.next()?;
    if ys.len() != 4
        || !(1..=2).contains(&ms.len())
        || !(1..=2).contains(&ds.len())
        || parts.next().is_some()
        || !ys
            .bytes()
            .chain(ms.bytes())
            .chain(ds.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut year: i64 = ys.parse().ok()?;
    let month: i64 = ms.parse().ok()?;
    let day: i64 = ds.parse().ok()?;
    // Date.UTC treats 0..99 as 1900..1999 and normalizes out-of-range components.
    if (0..=99).contains(&year) {
        year += 1900;
    }
    let mut hour = 0i64;
    let mut minute = 0i64;
    let mut second = 0i64;
    let mut millis = 0i64;
    let mut offset = 0i64;
    if let Some(time) = time {
        let zone_at = time.find(['Z', '+', '-']);
        let (clock, zone) = zone_at.map_or((time, None), |i| (&time[..i], Some(&time[i..])));
        let mut clock = clock.trim_end_matches([' ', '\t']).split(':');
        let hs = clock.next()?;
        let mins = clock.next()?;
        let secs = clock.next()?;
        if !(1..=2).contains(&hs.len())
            || !(1..=2).contains(&mins.len())
            || clock.next().is_some()
            || !hs.bytes().chain(mins.bytes()).all(|b| b.is_ascii_digit())
        {
            return None;
        }
        hour = hs.parse().ok()?;
        minute = mins.parse().ok()?;
        let (secs, frac) = secs
            .split_once('.')
            .map_or((secs, None), |(a, b)| (a, Some(b)));
        if !(1..=2).contains(&secs.len()) || !secs.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        second = secs.parse().ok()?;
        if let Some(frac) = frac {
            if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            millis = frac
                .bytes()
                .take(3)
                .fold(0, |n, b| n * 10 + i64::from(b - b'0'));
            for _ in frac.len()..3 {
                millis *= 10;
            }
        }
        if let Some(zone) = zone.filter(|&z| z != "Z") {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let zone = &zone[1..];
            let (h, m) = zone
                .split_once(':')
                .map_or((zone, None), |(h, m)| (h, Some(m)));
            if h.is_empty() || h.len() > 2 || !h.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let h: i64 = h.parse().ok()?;
            if h > 29 {
                return None;
            }
            let sexagesimal = if let Some(m) = m {
                if m.len() != 2 || !m.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                h * 60 + m.parse::<i64>().ok()?
            } else {
                h
            };
            // Match yaml 2.9.0's timestamp.resolve: parseSexagesimal(tz), then
            // multiply abs(d) < 30 by 60, including signed 00:mm offsets.
            offset = sign * sexagesimal;
            if offset.abs() < 30 {
                offset *= 60;
            }
        }
    }
    let month0 = month - 1;
    year += month0.div_euclid(12);
    let month = month0.rem_euclid(12) + 1;
    let days = days_from_civil(year, month, 1) + day - 1;
    let total_ms =
        days * 86_400_000 + (hour * 3600 + (minute - offset) * 60 + second) * 1000 + millis;
    let (year, month, day) = civil_from_days(total_ms.div_euclid(86_400_000));
    let rem = total_ms.rem_euclid(86_400_000);
    let date = if (0..=9999).contains(&year) {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        format!("{year:+07}-{month:02}-{day:02}")
    };
    Some(format!(
        "{date}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    ))
}

// Gregorian civil-day arithmetic; dates are bounded to four input year digits.
fn days_from_civil(mut y: i64, m: i64, d: i64) -> i64 {
    y -= i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    (y, m, d)
}
