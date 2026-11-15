use crate::SourceSpan;
use crate::diagrams::scan::{ScanStep, TokenScanner};

mod actor;
mod scanner;

use scanner::SequenceScanner;

#[cfg(test)]
use actor::HALF_ARROW_TYPES;

#[derive(Debug, Clone)]
pub(crate) enum Tok {
    Newline,

    SequenceDiagram,
    Participant,
    ActorKw,
    Create,
    Destroy,
    As,

    Box,
    End,

    Loop,
    Rect,
    Opt,
    Alt,
    Else,
    Par,
    ParOver,
    And,
    Critical,
    Option,
    Break,

    Note,
    LeftOf,
    RightOf,
    Over,

    Links,
    Link,
    Properties,
    Details,

    Autonumber,
    Off,

    Activate,
    Deactivate,

    Comma,
    Plus,
    Minus,
    Central,

    Num(f64),
    Actor(String),
    Text(String),
    RestOfLine(String),
    SignalType(i32),
    Config(String),

    Title(String),
    CompatTitle(String),
    AccTitle(String),
    AccDescr(String),
    AccDescrMultiline(String),
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub(crate) struct LexError {
    pub message: String,
    pub span: SourceSpan,
}

impl crate::error::ParseErrorSourceSpan for LexError {
    fn source_span(&self) -> Option<crate::SourceSpan> {
        Some(self.span)
    }
}

pub(super) struct Lexer<'input> {
    scanner: SequenceScanner<'input>,
}

impl<'input> Lexer<'input> {
    pub(super) fn new(input: &'input str) -> Self {
        Self {
            scanner: SequenceScanner::new(input),
        }
    }
}

impl TokenScanner for Lexer<'_> {
    type Event = std::result::Result<(usize, Tok, usize), LexError>;

    fn scan_step(&mut self) -> ScanStep<Self::Event> {
        self.scanner.scan_step()
    }
}

impl Iterator for Lexer<'_> {
    type Item = std::result::Result<(usize, Tok, usize), LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

#[cfg(test)]
mod tests {
    use super::{HALF_ARROW_TYPES, Lexer, Tok};

    #[test]
    fn empty_percent_comments_preserve_statement_boundaries() {
        for newline in ["\n", "\r\n"] {
            for comment in ["%%", "  %%", "%% comment", "# comment", "é%% comment"] {
                let input = format!(
                    "sequenceDiagram{newline}{comment}{newline}participant A{newline}{comment}"
                );
                let tokens: Vec<_> = Lexer::new(&input)
                    .map(|event| event.expect("comment or statement token").1)
                    .collect();
                assert_eq!(
                    tokens.iter().filter(|t| matches!(t, Tok::Newline)).count(),
                    3,
                    "{input:?}"
                );
                assert!(
                    tokens
                        .iter()
                        .any(|t| matches!(t, Tok::Actor(id) if id == "A"))
                );
            }
        }
    }

    #[test]
    fn invalid_config_does_not_repeat_without_consuming_input() {
        let mut lexer = Lexer::new("@{}");
        assert!(lexer.next().unwrap().is_err());
        assert!(lexer.take(8).count() < 8);
    }

    #[test]
    fn unterminated_actor_config_emits_one_error_and_stops_at_eof() {
        let input = "sequenceDiagram\nparticipant A@{ type: boundary\nparticipant B";
        let mut lexer = Lexer::new(input);
        let error = lexer
            .by_ref()
            .take(8)
            .find_map(Result::err)
            .expect("unterminated actor config diagnostic");
        assert_eq!(
            error.span,
            crate::SourceSpan::new(input.find("@{").unwrap(), input.len())
        );
        assert!(
            lexer.next().is_none(),
            "unclosed config consumes its remaining body"
        );
    }

    fn token_trace(input: &str) -> Vec<(usize, &'static str, usize)> {
        Lexer::new(input)
            .map(|event| {
                let (start, token, end) = event.expect("sequence token");
                let name = match token {
                    Tok::AccDescrMultiline(_) => "accDescrMultiline",
                    Tok::Newline => "newline",
                    Tok::Participant => "participant",
                    _ => "other",
                };
                (start, name, end)
            })
            .collect()
    }

    fn boundary_after_accessibility(input: &str) -> (usize, &'static str, usize) {
        let trace = token_trace(input);
        let accessibility = trace
            .iter()
            .position(|(_, name, _)| *name == "accDescrMultiline")
            .expect("multiline accessibility token");
        trace[accessibility + 1]
    }

    #[test]
    fn multiline_accessibility_only_synthesizes_required_statement_boundaries() {
        let ordinary = "sequenceDiagram\naccDescr {desc}\nparticipant A";
        let physical_newline = ordinary.find("}\n").expect("closing brace") + 1;
        assert_eq!(
            boundary_after_accessibility(ordinary),
            (physical_newline, "newline", physical_newline + 1)
        );

        let same_line = "sequenceDiagram\naccDescr {desc} participant A";
        let closing = same_line.find('}').expect("closing brace") + 1;
        assert_eq!(
            boundary_after_accessibility(same_line),
            (closing, "newline", closing)
        );

        let eof = "sequenceDiagram\naccDescr {desc}";
        assert_eq!(
            boundary_after_accessibility(eof),
            (eof.len(), "newline", eof.len())
        );
    }

    #[test]
    fn lexes_all_upstream_half_arrow_variants() {
        for (arrow, expected_type) in HALF_ARROW_TYPES {
            for input in [
                format!("A {arrow} B: message"),
                format!("A{arrow}B: message"),
                format!("顧客{arrow}サーバー: message"),
            ] {
                let signal_types: Vec<_> = Lexer::new(&input)
                    .map(|event| event.expect("sequence token").1)
                    .filter_map(|token| match token {
                        Tok::SignalType(signal_type) => Some(signal_type),
                        _ => None,
                    })
                    .collect();

                assert_eq!(signal_types, vec![expected_type], "{input}");
            }
        }
    }
}
