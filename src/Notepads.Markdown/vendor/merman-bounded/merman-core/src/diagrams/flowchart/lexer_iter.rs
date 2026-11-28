// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use super::{LexError, Lexer, Tok};

impl<'input> Iterator for Lexer<'input> {
    type Item = std::result::Result<(usize, Tok, usize), LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(tok) = self.pending.take() {
            return Some(tok);
        }

        if self.pos >= self.input.len() {
            return None;
        }

        // Mermaid's LINK and START_LINK rules precede NEWLINE/SPACE and include leading `\s*`.
        // Give an operator spanning line or Unicode whitespace the same longest-match priority.
        if let Some(res) = self.lex_arrow_and_label() {
            return Some(res);
        }

        if let Some(sep) = self.lex_sep() {
            self.allow_header_direction = false;
            return Some(Ok(sep));
        }
        self.skip_ws();
        if self.pos >= self.input.len() {
            return None;
        }
        if let Some(sep) = self.lex_sep() {
            self.allow_header_direction = false;
            return Some(Ok(sep));
        }
        if let Some(sep) = self.lex_comment() {
            self.allow_header_direction = false;
            return Some(Ok(sep));
        }
        self.skip_ws();
        if self.pos >= self.input.len() {
            return None;
        }

        let start = self.pos;
        if let Some(tok) = self.lex_direction_stmt() {
            return Some(tok);
        }
        if let Some(res) = self.lex_style_stmt() {
            return Some(res);
        }
        if let Some(res) = self.lex_classdef_stmt() {
            return Some(res);
        }
        if let Some(res) = self.lex_class_assign_stmt() {
            return Some(res);
        }
        if let Some(res) = self.lex_click_stmt() {
            return Some(res);
        }
        if let Some(res) = self.lex_link_style_stmt() {
            return Some(res);
        }
        if let Some(res) = self.lex_shape_data() {
            return Some(res);
        }
        if self.starts_with_kw("flowchart-elk") {
            self.pos += "flowchart-elk".len();
            self.allow_header_direction = true;
            return Some(Ok((start, Tok::KwFlowchartElk, self.pos)));
        }
        if self.starts_with_kw("swimlane-beta") {
            self.pos += "swimlane-beta".len();
            self.allow_header_direction = true;
            return Some(Ok((start, Tok::KwSwimlane, self.pos)));
        }
        if self.starts_with_kw("flowchart") {
            self.pos += "flowchart".len();
            self.allow_header_direction = true;
            return Some(Ok((start, Tok::KwFlowchart, self.pos)));
        }
        if self.starts_with_kw("graph") {
            self.pos += "graph".len();
            self.allow_header_direction = true;
            return Some(Ok((start, Tok::KwGraph, self.pos)));
        }
        if self.starts_with_kw("subgraph") {
            if self.region.is_some() {
                if self.native_subgraph_depth >= 8 {
                    return Some(Err(self.native_limit()));
                }
                self.native_subgraph_depth += 1;
            }
            self.pos += "subgraph".len();
            let keyword_end = self.pos;
            if let Some(header) = self.lex_subgraph_header_after_keyword(start) {
                self.pending = Some(header);
            }
            return Some(Ok((start, Tok::KwSubgraph, keyword_end)));
        }
        if self.starts_with_kw("end") {
            if self.region.is_some() {
                self.native_subgraph_depth = self.native_subgraph_depth.saturating_sub(1);
            }
            self.pos += "end".len();
            return Some(Ok((start, Tok::KwEnd, self.pos)));
        }

        if let Some(tok) = self.lex_style_sep() {
            return Some(Ok(tok));
        }

        if let Some(tok) = self.lex_direction() {
            return Some(tok);
        }
        self.allow_header_direction = false;

        if let Some(res) = self.lex_node_label() {
            return Some(res);
        }

        if let Some(tok) = self.lex_edge_id() {
            return Some(tok);
        }

        if let Some(tok) = self.lex_id() {
            return Some(tok);
        }

        if let Some(tok) = self.lex_amp() {
            return Some(Ok(tok));
        }

        // Keep recovery and error spans on UTF-8 boundaries after an unsupported character.
        self.pos += self.input[start..].chars().next().unwrap().len_utf8();
        Some(Err(if self.region.is_some() {
            LexError::terminal().at_span(crate::SourceSpan::new(start, self.pos))
        } else {
            LexError::with_span(
                format!("Unexpected character at {start}"),
                crate::SourceSpan::new(start, self.pos),
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_unicode_recovery_keeps_character_boundaries() {
        let mut lexer = Lexer::new("😀A");
        assert!(lexer.next().unwrap().is_err());
        let (start, token, end) = lexer.next().unwrap().unwrap();
        assert!(matches!(token, Tok::Id(ref id) if id == "A"));
        assert_eq!((start, end), (4, 5));
        assert!(lexer.next().is_none());
    }
}

#[cfg(test)]
mod native_lexer_tests {
    use super::*;
    use crate::OperationControl;
    use markdown_memory::{
        AllocationAuthority, Family, MemoryError, MemoryScope, NativePayload, NativeRegion,
    };
    use std::{
        alloc::Layout,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    struct Authority {
        used: AtomicUsize,
        reserves: AtomicUsize,
        allocates: AtomicUsize,
        fail_reserve: usize,
    }
    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, family: Family) -> Result<(), MemoryError> {
            if family == Family::Parse && layout.size() > 0 {
                let site = self.reserves.fetch_add(1, Ordering::SeqCst) + 1;
                if site == self.fail_reserve {
                    return Err(MemoryError::BudgetLimited);
                }
            }
            self.used.fetch_add(layout.size(), Ordering::SeqCst);
            Ok(())
        }
        fn release(&self, layout: Layout, _: Family) {
            self.used.fetch_sub(layout.size(), Ordering::SeqCst);
        }
        fn failure(&self) -> Option<MemoryError> {
            None
        }
        fn before_allocate(&self, _: Layout, family: Family) -> Result<(), MemoryError> {
            if family == Family::Parse {
                self.allocates.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    }
    fn scope(fail_reserve: usize) -> (MemoryScope, Arc<Authority>) {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            reserves: AtomicUsize::new(0),
            allocates: AtomicUsize::new(0),
            fail_reserve,
        });
        (MemoryScope::new(authority.clone()), authority)
    }
    const SOURCE: &str = "flowchart LR\nA[\"中文\"] -->|go| B{Stop}\nstyle A stroke-dasharray:5\\,5,fill:#fff\nclassDef hot stroke:#111\nclass A hot\nclick A href \"https://example.com\" \"tip\" _blank\nlinkStyle 0 interpolate linear stroke:#123, stroke-width:2px\nsubgraph s[Group]\nC\nend\n";

    #[test]
    fn native_lexer_preserves_fixed_unicode_arrow_style_and_interaction_baseline() {
        let (scope, authority) = scope(0);
        let region = NativeRegion::try_new(&scope, 256).unwrap();
        let control = OperationControl::new().with_memory_region(region.clone());
        let mut lexer = Lexer::controlled(SOURCE, &control);
        let mut tokens = Vec::new();
        for result in lexer.by_ref() {
            region
                .try_push(&mut tokens, result.unwrap(), Family::Parse)
                .unwrap();
        }
        assert!(tokens.iter().any(|(_, token, _)| matches!(token,
            Tok::NodeLabel(value) if value.text.text == "中文" && value.shape == "square")));
        assert!(tokens.iter().any(|(_, token, _)| matches!(token,
            Tok::Arrow(value) if value.link.end == "-->")));
        assert!(tokens.iter().any(|(_, token, _)| matches!(token,
            Tok::StyleStmt(value) if value.styles == ["stroke-dasharray:5,5", "fill:#fff"])));
        assert!(tokens.iter().any(|(_, token, _)| matches!(token,
            Tok::ClickStmt(value) if value.ids == ["A"] && value.tooltip.as_deref() == Some("tip"))));
        assert!(tokens.iter().any(|(_, token, _)| matches!(token,
            Tok::LinkStyleStmt(value) if value.styles == ["stroke:#123", " stroke-width:2px"])));
        let payload = NativePayload::new(tokens, region.clone());
        drop(lexer);
        drop(control);
        drop(region);
        assert!(authority.used.load(Ordering::SeqCst) > 0);
        drop(payload);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn zero_lexical_budget_refuses_owned_direction_before_payload_allocator() {
        let (scope, authority) = scope(1);
        let region = NativeRegion::try_new(&scope, 8).unwrap();
        let control = OperationControl::new().with_memory_region(region.clone());
        let mut lexer = Lexer::controlled("flowchart LR", &control);
        assert!(matches!(
            lexer.next().unwrap(),
            Ok((_, Tok::KwFlowchart, _))
        ));
        let error = lexer.next().unwrap().unwrap_err();
        assert!(error.message.is_empty());
        assert!(error.expected_syntax.is_empty());
        assert_eq!(authority.allocates.load(Ordering::SeqCst), 0);
        drop(lexer);
        drop(control);
        drop(region);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn each_lexical_nested_reservation_failure_drops_partial_tokens_and_all_receipts() {
        let (first_scope, authority) = scope(0);
        let region = NativeRegion::try_new(&first_scope, 256).unwrap();
        let control = OperationControl::new().with_memory_region(region.clone());
        for result in Lexer::controlled(SOURCE, &control) {
            assert!(result.is_ok());
        }
        let count = authority.reserves.load(Ordering::SeqCst);
        drop(control);
        drop(region);
        for site in 1..=count {
            let (scope, authority) = scope(site);
            let region = NativeRegion::try_new(&scope, 256).unwrap();
            let control = OperationControl::new().with_memory_region(region.clone());
            let mut failed = false;
            for result in Lexer::controlled(SOURCE, &control) {
                if let Err(error) = result {
                    assert!(error.message.is_empty());
                    failed = true;
                    break;
                }
            }
            assert!(
                failed,
                "injected lexical reservation {site} was not observed"
            );
            drop(control);
            drop(region);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
    }
}
