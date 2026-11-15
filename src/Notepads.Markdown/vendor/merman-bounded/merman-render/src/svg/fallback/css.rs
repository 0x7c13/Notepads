use crate::svg::pipeline::{checkpoint_loop, trim_with_checkpoints};

/// Reads the final declaration for one non-metric HTML fallback helper property.
/// Typography itself is resolved by `cascade`, which retains source context and
/// cascade priority.
pub(super) fn extract_style_property_with_checkpoints<E>(
    style: &str,
    property: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<String>, E> {
    let mut found = None;
    let mut declarations = StyleDeclarationScanner::new(style);
    let mut order = 0usize;
    while let Some(declaration) = declarations.next_with_checkpoints(checkpoint)? {
        checkpoint_loop(order, checkpoint)?;
        order = order.saturating_add(1);
        let Some(colon) = declaration.find(':') else {
            continue;
        };
        let name = trim_with_checkpoints(&declaration[..colon], checkpoint)?;
        if !name.eq_ignore_ascii_case(property) {
            continue;
        }
        let value = strip_important(&declaration[colon + 1..]);
        if !value.is_empty() {
            found = Some(value.to_string());
        }
    }
    checkpoint()?;
    Ok(found)
}

pub(super) struct StyleDeclarationScanner<'a> {
    style: &'a str,
    cursor: usize,
    finished: bool,
}

impl<'a> StyleDeclarationScanner<'a> {
    pub(super) const fn new(style: &'a str) -> Self {
        Self {
            style,
            cursor: 0,
            finished: false,
        }
    }

    pub(super) fn next_with_checkpoints<E>(
        &mut self,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<Option<&'a str>, E> {
        if self.finished {
            return Ok(None);
        }
        let start = self.cursor;
        let mut quote = None;
        let mut paren_depth = 0usize;
        for (iteration, (relative, character)) in self.style[start..].char_indices().enumerate() {
            checkpoint_loop(iteration, checkpoint)?;
            if let Some(current_quote) = quote {
                if character == current_quote {
                    quote = None;
                }
                continue;
            }
            match character {
                '\'' | '"' => quote = Some(character),
                '(' => paren_depth = paren_depth.saturating_add(1),
                ')' => paren_depth = paren_depth.saturating_sub(1),
                ';' if paren_depth == 0 => {
                    let end = start + relative;
                    self.cursor = end + character.len_utf8();
                    checkpoint()?;
                    return Ok(Some(&self.style[start..end]));
                }
                _ => {}
            }
        }
        self.finished = true;
        checkpoint()?;
        Ok(Some(&self.style[start..]))
    }
}

fn strip_important(value: &str) -> &str {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    let Some(offset) = lower.rfind("!important") else {
        return value;
    };
    if !lower[offset + "!important".len()..].trim().is_empty() {
        return value;
    }
    value[..offset].trim()
}

pub(super) fn parse_css_px_value(value: &str) -> Option<f64> {
    let value = strip_important(value);
    let number = value
        .strip_suffix("px")
        .or_else(|| value.strip_suffix("PX"))
        .unwrap_or(value)
        .trim();
    number
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_reads_later_declarations_without_reintroducing_selector_matching() {
        let mut checkpoint = || Ok::<(), std::convert::Infallible>(());
        let value = extract_style_property_with_checkpoints(
            "width: 10px; width: 20px !important;",
            "width",
            &mut checkpoint,
        )
        .unwrap();
        assert_eq!(value.as_deref(), Some("20px"));
    }
}
