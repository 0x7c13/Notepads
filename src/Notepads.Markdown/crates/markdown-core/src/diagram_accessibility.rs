// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Authored Mermaid accessibility statements, separate from the rendered scene.

use std::ops::Range;

pub const MAX_DIAGRAM_SOURCE_BYTES: usize = 16 * 1024;
const MAX_METADATA_STATEMENTS: usize = 16;

#[derive(Clone, Copy, Debug, Default)]
struct Span {
    start: usize,
    end: usize,
}

impl Span {
    fn range(self) -> Range<usize> {
        self.start..self.end
    }
}

#[derive(Debug, Default)]
pub struct DiagramAccessibility {
    title: Option<Span>,
    description: Option<Span>,
    statements: [Span; MAX_METADATA_STATEMENTS],
    count: usize,
    limited: bool,
}

impl DiagramAccessibility {
    pub fn title(&self) -> Option<Range<usize>> {
        self.title.map(Span::range)
    }

    pub fn description(&self) -> Option<Range<usize>> {
        self.description.map(Span::range)
    }

    pub fn statements(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.statements[..self.count]
            .iter()
            .map(|span| span.range())
    }

    pub fn is_limited(&self) -> bool {
        self.limited
    }
}

#[derive(Clone, Copy)]
enum Family {
    Flowchart,
    Members,
    State,
    Sequence,
    Pie,
}

impl Family {
    fn from_header(line: &str) -> Option<Self> {
        let name = line
            .trim_start_matches('\u{feff}')
            .split(|c: char| c.is_whitespace() || c == ';')
            .next()?;
        Some(match name {
            "flowchart" | "graph" => Self::Flowchart,
            "classDiagram" | "classDiagram-v2" | "erDiagram" => Self::Members,
            "stateDiagram" | "stateDiagram-v2" => Self::State,
            "sequenceDiagram" => Self::Sequence,
            "pie" => Self::Pie,
            _ => return None,
        })
    }
}

#[derive(Default)]
struct Context {
    quote: Option<char>,
    escaped: bool,
    depth: usize,
    scalar_data: bool,
}

impl Context {
    fn at_statement(&self) -> bool {
        self.quote.is_none() && self.depth == 0
    }

    fn observe(&mut self, line: &str, family: Family) {
        let members = matches!(family, Family::Members);
        if members {
            let statement = line.trim().trim_end_matches(';').trim_end();
            if statement.starts_with("%%") {
                return;
            }
            if self.depth != 0 {
                if statement.ends_with('}') {
                    self.depth = 0;
                }
                return;
            }
        }
        let mut previous = None;
        let mut last_token = None;
        let mut has_label = false;
        let mut in_alias = false;
        let mut alias_start = false;
        for (index, value) in line.char_indices() {
            if self.escaped {
                self.escaped = false;
                continue;
            }
            let can_escape = match self.quote {
                Some('"') => !matches!(family, Family::Flowchart) || self.scalar_data,
                None => matches!(family, Family::Sequence),
                _ => false,
            };
            if value == '\\' && can_escape {
                self.escaped = true;
                continue;
            }
            if let Some(quote) = self.quote {
                if value == quote {
                    self.quote = None;
                    last_token = Some(value);
                }
                continue;
            }
            let suffix = &line[index..];
            if !members
                && suffix.starts_with("%%")
                && (self.depth == 0 || !matches!(family, Family::Flowchart))
            {
                break;
            }
            if value == '"' && (!in_alias || alias_start)
                || value == '`' && members && !in_alias
                || value == '\'' && self.scalar_data
            {
                alias_start = false;
                self.quote = Some(value);
                continue;
            }
            if matches!(family, Family::Flowchart) {
                if suffix.starts_with("@{") {
                    self.scalar_data = true;
                }
                match value {
                    '[' | '(' | '{' => self.depth += 1,
                    '>' if self.depth == 0
                        && previous.is_some_and(|value: char| {
                            value.is_alphanumeric() || matches!(value, '_' | '.')
                        }) =>
                    {
                        self.depth += 1;
                    }
                    ']' | ')' | '}' => {
                        self.depth = self.depth.saturating_sub(1);
                        if value == '}' {
                            self.scalar_data = false;
                        }
                    }
                    _ => {}
                }
            }
            if members {
                match value {
                    '[' if !in_alias => {
                        in_alias = true;
                        alias_start = true;
                    }
                    ']' => in_alias = false,
                    _ if !value.is_whitespace() => alias_start = false,
                    _ => {}
                }
            }
            has_label |= value == ':' && !in_alias;
            if !value.is_whitespace() {
                previous = Some(value);
                if value != ';' {
                    last_token = Some(value);
                }
            }
        }
        if self.quote.is_none() {
            match (family, last_token) {
                (Family::Members | Family::State, Some('{')) if !has_label => self.depth += 1,
                (Family::State, Some('}')) if !has_label => {
                    self.depth = self.depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        // Class and ER parse physical rows, including arbitrary unquoted label
        // text. A quote cannot carry lexical state into the next row.
        if members {
            self.quote = None;
        }
        self.escaped = false;
    }
}

/// Collect byte coordinates for standalone Mermaid accessibility statements.
/// The last value for each kind wins. Authored metadata stays in the input and
/// export; callers may separately mask the admitted statements for rendering.
pub fn inspect<E>(
    source: &str,
    mut charge: impl FnMut(usize) -> Result<(), E>,
) -> Result<DiagramAccessibility, E> {
    let mut result = DiagramAccessibility::default();
    if source.len() > MAX_DIAGRAM_SOURCE_BYTES {
        result.limited = true;
        return Ok(result);
    }

    charge(source.len())?;
    let mut family = None;
    let mut context = Context::default();
    let mut at = 0;
    while at < source.len() {
        let end = source[at..]
            .find(['\r', '\n'])
            .map_or(source.len(), |n| at + n);
        let line = &source[at..end];
        charge(line.len() + 1)?;
        let significant = line.trim_start();
        let next = if source[end..].starts_with("\r\n") {
            end + 2
        } else {
            end + usize::from(end < source.len())
        };

        if let Some(family) = family {
            if context.at_statement()
                && let Some((is_title, delimiter)) = delimiter(significant)
            {
                let content_start = end - delimiter.len() + 1;
                if let Some((content_end, statement_end)) = if delimiter.starts_with(':') {
                    Some((end, next))
                } else {
                    let raw = &source[content_start..];
                    let close = raw.find('}');
                    charge(close.unwrap_or(raw.len()))?;
                    close.map(|offset| {
                        let close = content_start + offset;
                        (close, close + 1)
                    })
                } {
                    if result.count == MAX_METADATA_STATEMENTS {
                        result.limited = true;
                        break;
                    }
                    let raw = &source[content_start..content_end];
                    let value = raw.trim_start();
                    let start = content_end - value.len();
                    let end = start + value.trim_end().len();
                    let target = if is_title {
                        &mut result.title
                    } else {
                        &mut result.description
                    };
                    *target = (start < end).then_some(Span { start, end });
                    result.statements[result.count] = Span {
                        start: at,
                        end: statement_end,
                    };
                    result.count += 1;
                    at = statement_end;
                    continue;
                }
            }
            context.observe(line, family);
        } else if !significant.is_empty() && !significant.starts_with("%%") {
            family = Family::from_header(significant);
            if let Some(family) = family {
                context.observe(line, family);
            } else {
                break;
            }
        }
        at = next;
    }
    Ok(result)
}

fn delimiter(line: &str) -> Option<(bool, &str)> {
    let (is_title, tail) = line
        .strip_prefix("accTitle")
        .map(|tail| (true, tail))
        .or_else(|| line.strip_prefix("accDescr").map(|tail| (false, tail)))?;
    let tail = tail.trim_start();
    (tail.starts_with(':') || !is_title && tail.starts_with('{')).then_some((is_title, tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(source: &str) -> DiagramAccessibility {
        inspect(source, |_| Ok::<_, ()>(())).unwrap()
    }

    #[test]
    fn finds_authored_unicode_values_and_only_masks_top_level_statements() {
        let source = "%% heading\r\nflowchart TB\r\n  accTitle: 初稿\r\nA[\"inside\naccTitle: a label\"]\r\nD>two\naccDescr: part of a node]\n\
                      %% accTitle: comment\naccDescr {   短句\n%%{text in description}\n} A-->B\naccTitle: 最终版\n";
        let metadata = read(source);
        let value = |range: Option<Range<usize>>| range.map(|range| &source[range]);
        assert_eq!(value(metadata.title()), Some("最终版"));
        // The first closing brace is the end of the braced value; authored
        // punctuation and comment markers inside the value are opaque.
        assert_eq!(
            value(metadata.description()),
            Some("短句\n%%{text in description")
        );
        let statements: Vec<_> = metadata.statements().map(|range| &source[range]).collect();
        assert_eq!(statements.len(), 3);
        assert_eq!(statements[0], "  accTitle: 初稿\r\n");
        assert_eq!(statements[2], "accTitle: 最终版\n");
    }

    #[test]
    fn preserves_container_members_and_sequence_message_text() {
        for source in [
            "classDiagram\nclass Preview {\n    accTitle: member\n}\naccTitle: Title\n",
            "erDiagram\nORDER {\n    string accTitle \"member\"\n}\nORDER ||--o{ LINE : contains\naccTitle: Title\n",
            "stateDiagram\nstate \"quoted\naccTitle: diagram text\" as A\naccTitle: Title\n",
            "sequenceDiagram\nA->>B: \"message\naccTitle: diagram text\"\naccTitle: Title\n",
            "sequenceDiagram\nA->>B: say \\\"hello\naccTitle: Title\n",
            "classDiagram\nA --> B : open {\naccTitle: Title\n",
            "classDiagram\nclass A {\n+String key: value}\naccTitle: Title\n",
            "classDiagram\nclass A[He said \"] {\naccTitle: genuine class member\n}\naccTitle: Title\n",
            "classDiagram\nclass A {\n+String value = path\"}\naccTitle: Title\n",
            "erDiagram\nDisco{\n  string member \"key: value\"}\naccTitle: Title\n",
            "flowchart TB\nA@{shape:rect,label:'path\\'}\naccTitle: Title\n",
            "flowchart TB\nD >first line\naccTitle: label content]\naccTitle: Title\n",
        ] {
            let metadata = read(source);
            assert_eq!(metadata.title().map(|range| &source[range]), Some("Title"));
            assert_eq!(metadata.statements().count(), 1, "{source}");
        }
    }

    #[test]
    fn leaves_unknown_or_unclosed_metadata_to_the_renderer_and_has_a_fixed_bound() {
        let unknown = read("flowchart TB\naccTitleHint: prose\naccDescr { no close\nA-->B");
        assert_eq!(unknown.statements().count(), 0);
        let empty = read("pie\naccTitle: initial\naccTitle:\n");
        assert_eq!(empty.title(), None);
        assert_eq!(empty.statements().count(), 2);
        assert!(read(&format!("graph TD\n{}", "accTitle: x\n".repeat(17))).is_limited());
        assert!(read(&"x".repeat(MAX_DIAGRAM_SOURCE_BYTES + 1)).is_limited());
    }
}
