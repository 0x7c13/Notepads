pub fn count_occurrence(haystack: &str, needle: char) -> usize {
    haystack.chars().filter(|c| *c == needle).count()
}

fn should_combine_sets(previous_tilde_count: usize, next_tilde_count: usize) -> bool {
    previous_tilde_count == 1 && next_tilde_count == 1
}

#[derive(Debug, Clone, Copy)]
enum GenericTypeSet<'a> {
    Single { value: &'a str, tilde_count: usize },
    Combined { left: &'a str, right: &'a str },
}

impl<'a> GenericTypeSet<'a> {
    fn visit<E>(self, visit: &mut impl FnMut(&'a str) -> Result<(), E>) -> Result<(), E> {
        match self {
            Self::Single { value, tilde_count } => {
                visit_processed_generic_set(&[value], tilde_count, visit)
            }
            Self::Combined { left, right } => {
                visit_processed_generic_set(&[left, ",", right], 2, visit)
            }
        }
    }
}

fn visit_processed_generic_set<'a, E>(
    parts: &[&'a str],
    tilde_count: usize,
    visit: &mut impl FnMut(&'a str) -> Result<(), E>,
) -> Result<(), E> {
    if tilde_count <= 1 {
        for part in parts {
            visit(part)?;
        }
        return Ok(());
    }

    let preserve_leading_tilde =
        !tilde_count.is_multiple_of(2) && parts.first().is_some_and(|part| part.starts_with('~'));
    let paired_count = tilde_count / 2;
    let mut tilde_index = 0usize;
    for part in parts {
        let mut retained_start = 0usize;
        for (offset, ch) in part.char_indices() {
            if ch != '~' {
                continue;
            }
            if retained_start < offset {
                visit(&part[retained_start..offset])?;
            }
            let replacement = if preserve_leading_tilde && tilde_index == 0 {
                "~"
            } else {
                let paired_index = tilde_index - usize::from(preserve_leading_tilde);
                if paired_index < paired_count {
                    "<"
                } else if !preserve_leading_tilde
                    && !tilde_count.is_multiple_of(2)
                    && paired_index == paired_count
                {
                    "~"
                } else {
                    ">"
                }
            };
            visit(replacement)?;
            retained_start = offset + ch.len_utf8();
            tilde_index += 1;
        }
        if retained_start < part.len() {
            visit(&part[retained_start..])?;
        }
    }
    Ok(())
}

fn visit_generic_types<'a, E>(
    input: &'a str,
    visit: &mut impl FnMut(&'a str) -> Result<(), E>,
) -> Result<(), E> {
    let mut sets = input.split(',');
    let first = sets.next().unwrap_or_default();
    let mut previous = first;
    let mut previous_tilde_count = count_occurrence(first, '~');
    let mut pending = GenericTypeSet::Single {
        value: first,
        tilde_count: previous_tilde_count,
    };

    for next in sets {
        let next_tilde_count = count_occurrence(next, '~');
        if should_combine_sets(previous_tilde_count, next_tilde_count) {
            // Mermaid removes the previously emitted set before combining the two raw sets around
            // the comma. Keep the last set pending so that this replacement stays allocation-free.
            pending = GenericTypeSet::Combined {
                left: previous,
                right: next,
            };
        } else {
            pending.visit(visit)?;
            visit(",")?;
            pending = GenericTypeSet::Single {
                value: next,
                tilde_count: next_tilde_count,
            };
        }
        previous = next;
        previous_tilde_count = next_tilde_count;
    }

    pending.visit(visit)
}

/// An allocation-free plan for Mermaid's generic-type canonicalization.
#[derive(Debug, Clone, Copy)]
pub struct GenericTypesPlan<'a> {
    input: &'a str,
    output_len: usize,
    has_generic_syntax: bool,
}

impl<'a> GenericTypesPlan<'a> {
    pub fn new(input: &'a str) -> Self {
        if !input.contains('~') {
            return Self {
                input,
                output_len: input.len(),
                has_generic_syntax: false,
            };
        }

        let mut output_len = 0usize;
        let counted = visit_generic_types(input, &mut |fragment| {
            output_len += fragment.len();
            Ok::<(), std::convert::Infallible>(())
        });
        match counted {
            Ok(()) => {}
            Err(never) => match never {},
        }
        debug_assert!(output_len <= input.len());
        Self {
            input,
            output_len,
            has_generic_syntax: true,
        }
    }

    pub const fn output_len(self) -> usize {
        self.output_len
    }

    /// Returns a conservative work bound for revisiting the source during materialization.
    pub fn materialization_scan_work(self) -> Option<usize> {
        if self.has_generic_syntax {
            self.input.len().max(1).checked_mul(2)
        } else {
            Some(0)
        }
    }

    /// Visits the canonical text without allocating an intermediate string.
    /// Visits canonical fragments and stops at the first callback error.
    pub fn try_visit<E>(self, mut visit: impl FnMut(&'a str) -> Result<(), E>) -> Result<(), E> {
        if !self.has_generic_syntax {
            visit(self.input)
        } else {
            visit_generic_types(self.input, &mut visit)
        }
    }

    /// Visits canonical fragments without allocating the resulting string.
    pub fn visit(self, mut visit: impl FnMut(&'a str)) {
        let visited = self.try_visit(|fragment| {
            visit(fragment);
            Ok::<(), std::convert::Infallible>(())
        });
        match visited {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }
}

pub fn parse_generic_types(input: &str) -> String {
    // Mirrors Mermaid's `parseGenericTypes` logic (packages/mermaid/src/diagrams/common/common.ts).
    let plan = GenericTypesPlan::new(input);
    let mut output = String::with_capacity(plan.output_len());
    plan.visit(|fragment| output.push_str(fragment));
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_generic_types_matches_upstream_examples() {
        let cases = [
            ("test~T~", "test<T>"),
            ("test~Array~Array~string~~~", "test<Array<Array<string>>>"),
            (
                "test~Array~Array~string[]~~~",
                "test<Array<Array<string[]>>>",
            ),
            (
                "test ~Array~Array~string[]~~~",
                "test <Array<Array<string[]>>>",
            ),
            ("~test", "~test"),
            ("~test~T~", "~test<T>"),
            ("Map~K,V~", "Map<K,V>"),
            ("A~,B~,C~", "B<,C>"),
            ("foo~A~B~", "foo<A~B>"),
            ("~foo~A~", "~foo<A>"),
            (",~T~", ",<T>"),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_generic_types(input), expected);

            let plan = GenericTypesPlan::new(input);
            let mut visited = String::new();
            plan.visit(|fragment| visited.push_str(fragment));
            assert_eq!(visited, expected);
            assert_eq!(plan.output_len(), expected.len());
        }
    }
}
