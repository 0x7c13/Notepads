// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::error::LimitKind;
use crate::identity::SourceKey;
use crate::policy::{CancellationToken, SourceBudget};
use crate::source::SourceDocument;
use std::sync::Arc;

fn model(root: &SourceBudget, text: &str, revision: u64) -> AcceptedModel {
    let mut op = root.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([214; 16], revision).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(source.snapshot(), ParseConfig::new(Profile::G3), 1, &mut op).unwrap()
}

fn snapshot(model: AcceptedModel) -> HighlightSnapshot {
    let mut op = model.budget().begin(CancellationToken::new());
    HighlightSnapshot::build(model, &mut op).unwrap()
}

fn text(model: &AcceptedModel, range: crate::identity::LogicalSpan) -> String {
    let mut result = String::new();
    let mut op = model.budget().begin(CancellationToken::new());
    crate::reader::text::visit(model, model.main_flow_id(), range, &mut op, |value, _| {
        result.push_str(value);
        Ok(())
    })
    .unwrap();
    result
}

fn assert_bounds(snapshot: &HighlightSnapshot) {
    for row in snapshot.rows() {
        assert_eq!(
            snapshot.model().ir().nodes()[row.node.index()].flow,
            snapshot.model().main_flow_id()
        );
        let start = row.spans.start as usize;
        let end = start + row.spans.len as usize;
        assert!(end <= snapshot.spans().len());
        if row.status != BlockStatus::Complete {
            assert_eq!(start, end);
            continue;
        }
        let code = text(snapshot.model(), row.logical);
        let mut boundaries = vec![false; code.encode_utf16().count() + 1];
        let mut units = 0;
        boundaries[0] = true;
        for scalar in code.chars() {
            units += scalar.len_utf16();
            boundaries[units] = true;
        }
        let mut position = 0;
        for span in &snapshot.spans()[start..end] {
            assert_eq!(span.start_utf16 as usize, position);
            assert!(span.length_utf16 != 0);
            position += span.length_utf16 as usize;
            assert!(boundaries[position]);
        }
        assert_eq!(position, code.encode_utf16().count());
    }
}

#[test]
fn real_g3_allowlist_uses_fence_info_and_keeps_candidates_plain() {
    let root = SourceBudget::preview_without_deadline_for_test();
    let languages = [
        "cpp",
        "c++",
        "cxx",
        "rust",
        "rs",
        "json",
        "js",
        "javascript",
        "py",
        "python",
        "CS",
        "c#",
        "c",
        "hpp",
        "python3",
        "unknown",
    ];
    let source: String = languages
        .into_iter()
        .map(|language| format!("```{language} extra\n42\n```\n\n"))
        .collect();
    let model = model(&root, &source, 1);
    let snapshot = snapshot(model.clone());
    assert_eq!(snapshot.identity(), model.identity());
    assert_eq!(snapshot.summary().complete_blocks, 15);
    assert_eq!(snapshot.summary().skipped_blocks, 1);
    assert_eq!(snapshot.rows().len(), languages.len());
    assert_eq!(model.source().text(), source);
    assert_eq!(snapshot.rows()[13].language, Some(Language::Cpp));
    assert_eq!(snapshot.rows()[13].status, BlockStatus::Complete);
    assert_eq!(snapshot.rows()[12].language, Some(Language::C));
    assert_eq!(snapshot.rows()[12].status, BlockStatus::Complete);
    for index in [10, 11] {
        assert_eq!(snapshot.rows()[index].language, Some(Language::CSharp));
        assert_eq!(snapshot.rows()[index].status, BlockStatus::Complete);
    }
    assert_eq!(snapshot.rows()[14].language, Some(Language::Python));
    assert_eq!(snapshot.rows()[14].status, BlockStatus::Complete);
    assert_eq!(
        snapshot.rows()[15].status,
        BlockStatus::Skipped(SkipReason::UnknownLanguage)
    );
    assert_bounds(&snapshot);
}

#[test]
fn cpp_fence_aliases_preserve_canonical_tokens_and_unicode_text() {
    let root = SourceBudget::preview();
    let body = "// 中😀\nconst char* s = \"😀\";\nint n = 42;\n";
    let aliases = ["cpp", "cc", "hpp", "hh", "hxx", "h++", "CC", "HpP"];
    let source: String = aliases
        .iter()
        .map(|alias| {
            format!(
                "```{alias} extra\r\n{}```\r\n\r\n",
                body.replace('\n', "\r\n")
            )
        })
        .collect();
    let model = model(&root, &source, 1);
    let snapshot = snapshot(model.clone());
    assert_eq!(snapshot.summary().complete_blocks as usize, aliases.len());
    assert_eq!(snapshot.summary().skipped_blocks, 0);
    assert_eq!(snapshot.summary().failed_blocks, 0);
    let canonical = &snapshot.spans()[..snapshot.rows()[0].spans.len as usize];
    assert!(
        canonical
            .iter()
            .any(|span| span.kind == SemanticKind::Comment)
    );
    assert!(
        canonical
            .iter()
            .any(|span| span.kind == SemanticKind::String)
    );
    assert!(
        canonical
            .iter()
            .any(|span| span.kind == SemanticKind::Number)
    );
    for row in snapshot.rows() {
        assert_eq!(row.language, Some(Language::Cpp));
        assert_eq!(row.status, BlockStatus::Complete);
        assert_eq!(text(&model, row.logical), body);
        let start = row.spans.start as usize;
        assert_eq!(
            &snapshot.spans()[start..start + row.spans.len as usize],
            canonical
        );
    }
    assert_eq!(model.source().text(), source);
    assert_bounds(&snapshot);
}

#[test]
fn additional_fence_languages_color_their_own_syntax() {
    let root = SourceBudget::preview_without_deadline_for_test();
    for (aliases, language, body, tokens) in [
        (
            &["csharp", "cs", "C#"][..],
            Language::CSharp,
            "// 中😀\npublic class Sample { string label = \"示例\"; }\n",
            [
                ("中", SemanticKind::Comment),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["jsonc", "JSONC"][..],
            Language::Json,
            "{\n  // 中文 😀\n  \"count\": 42, /* 尾注 */\n}\n",
            [
                ("尾注", SemanticKind::Comment),
                ("42", SemanticKind::Number),
            ],
        ),
        (
            &["html", "HTM"][..],
            Language::Html,
            "<!-- 中😀 -->\n<strong title=\"示例\">Text</strong>\n",
            [
                ("strong", SemanticKind::Type),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["xhtml", "XHTML"][..],
            Language::Html,
            "<!-- 中文 😀 -->\n<br title=\"示例\" />\n",
            [("br", SemanticKind::Type), ("示例", SemanticKind::String)],
        ),
        (
            &["css"][..],
            Language::Css,
            "/* 中😀 */\n.sample { width: 42px; }\n",
            [("width", SemanticKind::Type), ("42", SemanticKind::Number)],
        ),
        (
            &["sql"][..],
            Language::Sql,
            "-- 中😀\nSELECT '示例' FROM samples;\n",
            [
                ("SELECT", SemanticKind::Keyword),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["go", "golang"][..],
            Language::Go,
            "func Sample() string { return \"中😀\" }\n",
            [
                ("Sample", SemanticKind::Function),
                ("return", SemanticKind::Keyword),
            ],
        ),
        (
            &["java"][..],
            Language::Java,
            "public class Sample { String label = \"中😀\"; }\n",
            [("Sample", SemanticKind::Type), ("中", SemanticKind::String)],
        ),
        (
            &["diff", "udiff", "patch"][..],
            Language::Diff,
            "--- a/sample\n+++ b/sample\n@@ -1 +1 @@\n-旧版本😀\n+新版本😀\n",
            [("-旧", SemanticKind::Removed), ("+新", SemanticKind::Added)],
        ),
        (
            &["ruby", "rb"][..],
            Language::Ruby,
            "# 中😀\ndef sample\n  \"示例\"\nend\n",
            [
                ("sample", SemanticKind::Function),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["xml"][..],
            Language::Xml,
            "<!-- 中😀 -->\n<sample title=\"示例\" />\n",
            [
                ("sample", SemanticKind::Type),
                ("title", SemanticKind::Keyword),
            ],
        ),
        (
            &["lua"][..],
            Language::Lua,
            "-- 中😀\nlocal function preview()\n  return \"示例\"\nend\n",
            [
                ("preview", SemanticKind::Function),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["makefile", "make"][..],
            Language::Makefile,
            "# 中😀\npreview: input.txt\n\t@echo $(TITLE)\n",
            [
                ("preview", SemanticKind::Function),
                ("echo", SemanticKind::Function),
            ],
        ),
        (
            &["c", "C"][..],
            Language::C,
            "#include <stdio.h>\n/* 中😀 */\nstatic int preview(void) { return 42; }\n",
            [
                ("#include", SemanticKind::Keyword),
                ("42", SemanticKind::Number),
            ],
        ),
        (
            &["r", "Rscript"][..],
            Language::R,
            "# 中😀\npreview <- function() { print(\"示例\") }\n",
            [
                ("preview", SemanticKind::Function),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["scala"][..],
            Language::Scala,
            "// 中😀\nobject Preview {\n  def label: String = \"示例\"\n}\n",
            [
                ("Preview", SemanticKind::Type),
                ("label", SemanticKind::Function),
            ],
        ),
        (
            &["md", "markdown"][..],
            Language::Markdown,
            "# 标题 😀\n\n**bold** and *italic* plus `code`.\n",
            [
                ("标题", SemanticKind::Type),
                ("bold", SemanticKind::Keyword),
            ],
        ),
        (
            &["bat", "batchfile", "batch", "dosbatch", "winbatch"][..],
            Language::Batchfile,
            "@echo off\nrem 中文 😀\nset \"LABEL=示例\"\nexit /b 0\n",
            [
                ("中文", SemanticKind::Comment),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["perl"][..],
            Language::Perl,
            "# 中文 😀\nsub label { return \"示例\"; }\n",
            [
                ("label", SemanticKind::Function),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["haskell"][..],
            Language::Haskell,
            "-- 中文 😀\nmodule Preview where\nlabel :: String\nlabel = \"示例\"\n",
            [
                ("module", SemanticKind::Keyword),
                ("String", SemanticKind::Type),
            ],
        ),
        (
            &["objective-c", "obj-c", "objc", "objectivec"][..],
            Language::ObjectiveC,
            "// 中文 😀\nNSString *label = @\"示例\";\n",
            [
                ("NSString", SemanticKind::Type),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["objective-c++", "obj-c++", "objc++", "objectivec++"][..],
            Language::ObjectiveCpp,
            "// 中文 😀\n#include <vector>\nstd::vector<int> widths = {42};\nNSString *label = @\"示例\";\n",
            [("42", SemanticKind::Number), ("示例", SemanticKind::String)],
        ),
        (
            &["php"][..],
            Language::Php,
            "// 中文 😀\nfunction label(): string { return \"<?php 示例\"; }\n",
            [
                ("label", SemanticKind::Function),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["php"][..],
            Language::Php,
            "\n  <?php\n// 中文 😀\nfunction label(): string { return \"示例\"; }\n",
            [
                ("<?php", SemanticKind::Keyword),
                ("label", SemanticKind::Function),
            ],
        ),
        (
            &["html+php", "phtml", "PHTML"][..],
            Language::HtmlPhp,
            "<!-- 中文 😀 -->\n<section><?= label() ?></section>\n",
            [
                ("section", SemanticKind::Type),
                ("label", SemanticKind::Function),
            ],
        ),
        (
            &["tex", "latex"][..],
            Language::Tex,
            "% 中文 😀\n\\documentclass{article}\n\\section{预览}\n",
            [
                ("article", SemanticKind::Type),
                ("section", SemanticKind::Function),
            ],
        ),
        (
            &["svg"][..],
            Language::Xml,
            "<!-- 中文 😀 -->\n<svg viewBox=\"0 0 40 40\"><circle r=\"10\" /></svg>\n",
            [
                ("circle", SemanticKind::Type),
                ("viewBox", SemanticKind::Keyword),
            ],
        ),
        (
            &["clojure", "clj"][..],
            Language::Clojure,
            "; 中文 😀\n(defn label [name] (str \"示例\" name))\n",
            [
                ("label", SemanticKind::Function),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["erlang", "erl"][..],
            Language::Erlang,
            "% 中文 😀\n-module(preview).\nlabel() -> \"示例\".\n",
            [
                ("module", SemanticKind::Keyword),
                ("label", SemanticKind::Function),
            ],
        ),
        (
            &["groovy", "gradle"][..],
            Language::Groovy,
            "// 中文 😀\ndef label = '示例'\nprintln(label)\n",
            [
                ("中文", SemanticKind::Comment),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["ocaml", "OCaml"][..],
            Language::Ocaml,
            "(* 中文 😀 *)\nlet label = \"示例\"\n",
            [
                ("let", SemanticKind::Keyword),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["lisp", "common-lisp"][..],
            Language::CommonLisp,
            "; 中文 😀\n(defun label () \"示例\")\n",
            [
                ("label", SemanticKind::Function),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["tcl"][..],
            Language::Tcl,
            "# 中文 😀\nset label \"示例\"\nputs $label\n",
            [
                ("中文", SemanticKind::Comment),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["properties", "java-properties"][..],
            Language::JavaProperties,
            "# 中文 😀\npreview.name=示例\npreview.width=42\n",
            [
                ("中文", SemanticKind::Comment),
                ("preview.name", SemanticKind::Keyword),
            ],
        ),
        (
            &["dot", "gv"][..],
            Language::Graphviz,
            "// 中文 😀\ndigraph { start -> end [label=\"示例\"]; }\n",
            [
                ("中文", SemanticKind::Comment),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["matlab", "MATLAB", "octave"][..],
            Language::Matlab,
            "% 中文 😀\nx = 42;\ndisp('示例');\n",
            [
                ("中文", SemanticKind::Comment),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["pascal", "objectpascal", "delphi"][..],
            Language::Pascal,
            "{ 中文 😀 }\nbegin\n  WriteLn('示例');\nend.\n",
            [
                ("中文", SemanticKind::Comment),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["rst", "reStructuredText"][..],
            Language::RestructuredText,
            ".. 中文 😀\n\n.. note::\n\n   示例\n",
            [
                ("中文", SemanticKind::Comment),
                ("note", SemanticKind::Keyword),
            ],
        ),
        (
            &["AppleScript", "apples", "osascript"][..],
            Language::AppleScript,
            "-- 中文 😀\nset labelText to \"示例\"\nreturn labelText\n",
            [
                ("中文", SemanticKind::Comment),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["d", "Dlang"][..],
            Language::D,
            "/+ 中文 😀 +/\nimmutable string label = \"示例\";\n",
            [
                ("中文", SemanticKind::Comment),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["haml"][..],
            Language::Haml,
            "/ 中文 😀\n%section.panel\n  %p= \"示例\"\n",
            [
                ("中文", SemanticKind::Comment),
                ("section", SemanticKind::Type),
            ],
        ),
        (
            &["html+erb", "erb", "rhtml", "html+ruby"][..],
            Language::HtmlErb,
            "<%# 中文 😀 %>\n<strong><%= \"示例\" %></strong>\n",
            [
                ("strong", SemanticKind::Type),
                ("示例", SemanticKind::String),
            ],
        ),
        (
            &["ts", "TypeScript", "mts", "cts"][..],
            Language::TypeScript,
            "interface User { name: string }\nconst ready: boolean = true;\n",
            [("interface", SemanticKind::Type), ("true", SemanticKind::Keyword)],
        ),
        (
            &["tsx", "TypeScriptReact"][..],
            Language::TypeScriptReact,
            "const node = <section title={name}>你好 {name}</section>;\n",
            [("section", SemanticKind::Type), ("title", SemanticKind::Keyword)],
        ),
        (
            &["powershell", "pwsh", "ps1", "psm1", "psd1"][..],
            Language::PowerShell,
            "# 参数示例\nfunction Get-Example { Write-Output \"你好\" }\n",
            [("参数", SemanticKind::Comment), ("你好", SemanticKind::String)],
        ),
        (
            &["toml", "TOML"][..],
            Language::Toml,
            "[project]\nname = \"示例\"\nenabled = true\n",
            [("project", SemanticKind::Type), ("true", SemanticKind::Keyword)],
        ),
        (
            &["dockerfile", "docker", "Containerfile"][..],
            Language::Dockerfile,
            "FROM alpine:3.22 AS build\nRUN echo \"ready\"\n",
            [("FROM", SemanticKind::Keyword), ("ready", SemanticKind::String)],
        ),
    ] {
        for alias in aliases {
            let source = format!("```{alias}\r\n{}```\r\n", body.replace('\n', "\r\n"));
            let model = model(&root, &source, 1);
            let snapshot = snapshot(model.clone());
            let row = snapshot.rows()[0];
            assert_eq!(row.language, Some(language), "{alias}");
            assert_eq!(row.status, BlockStatus::Complete, "{alias}");
            assert_eq!(text(&model, row.logical), body, "{alias}");
            for (needle, expected) in tokens {
                let start = body[..body.find(needle).unwrap()].encode_utf16().count() as u32;
                let token = snapshot
                    .spans()
                    .iter()
                    .find(|span| {
                        span.start_utf16 <= start && start < span.start_utf16 + span.length_utf16
                    })
                    .unwrap();
                assert_eq!(token.kind, expected, "{alias} {needle}");
            }
            assert_bounds(&snapshot);
        }
    }
}

#[test]
fn yaml_fences_have_real_key_boolean_string_number_and_comment_roles() {
    let root = SourceBudget::preview();
    let body = "# 中😀\nname: \"😀\"\ncount: 42\nenabled: true\n";
    for alias in ["yaml", "yml", "YAML"] {
        let source = format!("```{alias} extra\r\n{}```\r\n", body.replace('\n', "\r\n"));
        let model = model(&root, &source, 1);
        let snapshot = snapshot(model.clone());
        let row = snapshot.rows()[0];
        assert_eq!(row.language, Some(Language::Yaml));
        assert_eq!(row.status, BlockStatus::Complete);
        assert_eq!(text(&model, row.logical), body);
        for (needle, expected) in [
            ("中", SemanticKind::Comment),
            ("name", SemanticKind::Type),
            ("\"😀\"", SemanticKind::String),
            ("42", SemanticKind::Number),
            ("true", SemanticKind::Keyword),
        ] {
            let position = body[..body.find(needle).unwrap()].encode_utf16().count() as u32;
            let token = snapshot
                .spans()
                .iter()
                .find(|span| {
                    span.start_utf16 <= position && position < span.start_utf16 + span.length_utf16
                })
                .unwrap();
            assert_eq!(token.kind, expected, "{alias} {needle}");
        }
        assert_eq!(model.source().text(), source);
        assert_bounds(&snapshot);
    }
}

#[test]
fn shell_aliases_share_bash_tokens_and_preserve_the_lab_printf_literal() {
    let body = "# 中😀\nprintf '%s\\n' 'Markdown literal * * *'\nif true; then\n    echo \"中😀 $value\"\nfi\n";
    let aliases = [
        "sh",
        "bash",
        "shell",
        "shell-script",
        "SH",
        "BaSh",
        "SHELL",
        "zsh",
        "envrc",
        "KSH",
    ];
    let source: String = aliases
        .iter()
        .map(|alias| {
            format!(
                "```{alias} extra\r\n{}```\r\n\r\n",
                body.replace('\n', "\r\n")
            )
        })
        .collect();
    let model = model(&SourceBudget::preview(), &source, 1);
    let snapshot = snapshot(model.clone());
    assert_eq!(snapshot.summary().complete_blocks as usize, aliases.len());
    assert_eq!(snapshot.summary().skipped_blocks, 0);
    assert_eq!(snapshot.summary().failed_blocks, 0);
    let canonical = &snapshot.spans()[..snapshot.rows()[0].spans.len as usize];
    for (needle, expected) in [
        ("中", SemanticKind::Comment),
        ("printf", SemanticKind::Function),
        ("Markdown literal", SemanticKind::String),
        ("if", SemanticKind::Keyword),
    ] {
        let position = body[..body.find(needle).unwrap()].encode_utf16().count() as u32;
        let token = canonical
            .iter()
            .find(|span| {
                span.start_utf16 <= position && position < span.start_utf16 + span.length_utf16
            })
            .unwrap();
        assert_eq!(token.kind, expected, "{needle}");
    }
    for row in snapshot.rows() {
        assert_eq!(row.language, Some(Language::Shell));
        assert_eq!(row.status, BlockStatus::Complete);
        assert_eq!(text(&model, row.logical), body);
        let start = row.spans.start as usize;
        assert_eq!(
            &snapshot.spans()[start..start + row.spans.len as usize],
            canonical
        );
    }
    assert_eq!(model.source().text(), source);
    assert_bounds(&snapshot);
}

#[test]
fn shell_heredocs_and_unclosed_strings_keep_scalar_bounds_and_source() {
    for body in [
        "cat <<'EOF'\n中😀 *literal*\nEOF\nprintf '%s\\n' \"$value\"\n",
        "printf '%s\\n' \"unfinished 中😀\n",
    ] {
        let source = format!("```sh\n{body}```\n");
        let model = model(&SourceBudget::preview(), &source, 1);
        let snapshot = snapshot(model.clone());
        assert_eq!(snapshot.rows()[0].status, BlockStatus::Complete);
        assert_eq!(text(&model, snapshot.rows()[0].logical), body);
        assert_eq!(model.source().text(), source);
        assert_bounds(&snapshot);
    }
}

#[test]
fn shell_aliases_keep_the_line_limit_and_unknown_languages_stay_plain() {
    for alias in ["fish", "no-such-language"] {
        let source = format!("```{alias}\nprintf 'plain'\n```\n");
        let model = model(&SourceBudget::preview(), &source, 1);
        let snapshot = snapshot(model);
        assert_eq!(
            snapshot.rows()[0].status,
            BlockStatus::Skipped(SkipReason::UnknownLanguage)
        );
        assert_bounds(&snapshot);
    }
    let oversized = "x".repeat(MAX_LINE_UTF8 + 1);
    for alias in ["sh", "zsh", "envrc", "KSH"] {
        let source = format!("```{alias}\n{oversized}\n```\n");
        let snapshot = snapshot(model(&SourceBudget::preview(), &source, 1));
        assert_eq!(
            snapshot.rows()[0].language,
            Some(Language::Shell),
            "{alias}"
        );
        assert_eq!(
            snapshot.rows()[0].status,
            BlockStatus::Skipped(SkipReason::LineBytes),
            "{alias}"
        );
        assert_bounds(&snapshot);
    }
}

#[test]
fn unicode_crlf_spans_match_semantic_main_not_source_fence_offsets() {
    let root = SourceBudget::preview();
    let source = "paragraph\r\n\r\n```RuSt\r\n// 😀\r\nlet 名 = \"😀\";\r\nlet n = 42;\r\n```\r\n";
    let model = model(&root, source, 1);
    let snapshot = snapshot(model.clone());
    let row = snapshot.rows()[0];
    assert_eq!(row.language, Some(Language::Rust));
    assert_eq!(
        text(&model, row.logical),
        "// 😀\nlet 名 = \"😀\";\nlet n = 42;\n"
    );
    assert_ne!(row.logical.start(), source.find("//").unwrap() as u64);
    let kind = |position: u32| {
        snapshot
            .spans()
            .iter()
            .find(|span| {
                span.start_utf16 <= position && position < span.start_utf16 + span.length_utf16
            })
            .unwrap()
            .kind
    };
    assert_eq!(kind(3), SemanticKind::Comment);
    assert_eq!(kind(15), SemanticKind::String);
    assert_eq!(kind(28), SemanticKind::Number);
    assert_eq!(model.source().text(), source);
    assert_bounds(&snapshot);
}

#[test]
fn multiline_state_survives_lines_and_no_info_is_explicitly_skipped() {
    let root = SourceBudget::preview();
    let snapshot = snapshot(model(
        &root,
        "```cpp\n/* start\ninside\n*/\nint n=1;\n```\n\n```\nplain\n```\n",
        1,
    ));
    let code = text(snapshot.model(), snapshot.rows()[0].logical);
    let position = code[..code.find("inside").unwrap()].encode_utf16().count() as u32;
    assert_eq!(
        snapshot
            .spans()
            .iter()
            .find(|span| span.start_utf16 <= position
                && position < span.start_utf16 + span.length_utf16)
            .unwrap()
            .kind,
        SemanticKind::Comment
    );
    assert_eq!(
        snapshot.rows()[1].status,
        BlockStatus::Skipped(SkipReason::UnknownLanguage)
    );
    assert_bounds(&snapshot);
}

#[test]
fn fixed_block_line_and_entry_limits_leave_original_main_readable() {
    let root = SourceBudget::preview();
    for (body, reason) in [
        ("x".repeat(MAX_BLOCK_UTF8 + 1), SkipReason::BlockBytes),
        ("x\n".repeat(MAX_BLOCK_LINES + 1), SkipReason::BlockLines),
        ("x".repeat(MAX_LINE_UTF8 + 1), SkipReason::LineBytes),
    ] {
        let model = model(&root, &format!("```rs\n{body}\n```\n"), 1);
        let snapshot = snapshot(model.clone());
        assert_eq!(snapshot.rows()[0].status, BlockStatus::Skipped(reason));
        assert!(snapshot.spans().is_empty());
        assert!(text(&model, snapshot.rows()[0].logical).contains(&body));
    }
    let snapshot = snapshot(model(
        &root,
        &"```rs\n42\n```\n\n".repeat(MAX_BLOCKS + 1),
        2,
    ));
    assert_eq!(snapshot.rows().len(), MAX_BLOCKS);
    assert_eq!(snapshot.summary().status, SnapshotStatus::EntryLimit);
    assert_bounds(&snapshot);
}

#[test]
fn participating_code_bytes_are_cumulative_across_blocks() {
    let root = SourceBudget::preview();
    let line = format!("//{}\n", "x".repeat(4093));
    assert_eq!(line.len(), 4096);
    let body = line.repeat(8);
    let model = model(&root, &format!("```rs\n{body}```\n\n").repeat(33), 1);
    let mut op = root.begin(CancellationToken::new());
    // This is an accounting unit control, not a language compatibility claim. The actual
    // long-comment backend difference is preserved, with the same source, in the next test.
    let (buffers, summary) = build::build_with(&model, &mut op, |_, _, expected, work, spans| {
        assert!(work.with_op(|op| spans.push(
            TokenSpan {
                start_utf16: 0,
                length_utf16: expected,
                kind: SemanticKind::Plain
            },
            op
        ))?);
        Ok(engine::Outcome::Complete)
    })
    .unwrap();
    let snapshot = HighlightSnapshot {
        buffers,
        model,
        summary,
    };
    assert_eq!(
        snapshot.summary().participating_utf8,
        MAX_PARTICIPATING_UTF8 as u64
    );
    assert_eq!(
        snapshot.rows()[32].status,
        BlockStatus::Skipped(SkipReason::DocumentBytes)
    );
    assert_eq!(snapshot.summary().complete_blocks, 32);
    assert_bounds(&snapshot);
}

#[test]
fn ordinary_long_rust_comments_keep_the_real_compatibility_difference_visible() {
    // Original compatibility expectation is 32 Complete blocks; actual is 32 FailedRegexSearch.
    // Do not count this safety/fallback assertion as passing Rust compatibility. The original
    // input and expectation remain explicit rather than replacing the grammar or generated oracle.
    const EXPECTED_COMPLETE_BLOCKS: u32 = 32;
    // This oracle measures the regex fallback and byte ceiling, independent of runner load.
    let root = SourceBudget::preview_without_deadline_for_test();
    let line = format!("//{}\n", "x".repeat(4093));
    let body = line.repeat(8);
    let model = model(&root, &format!("```rs\n{body}```\n\n").repeat(33), 1);
    let snapshot = snapshot(model.clone());
    assert_ne!(snapshot.summary().complete_blocks, EXPECTED_COMPLETE_BLOCKS);
    assert_eq!(snapshot.summary().failed_blocks, 32);
    assert_eq!(snapshot.summary().complete_blocks, 0);
    for row in &snapshot.rows()[..32] {
        assert_eq!(row.status, BlockStatus::Failed(FailureReason::RegexSearch));
        assert_eq!(row.spans.len, 0);
    }
    assert_eq!(
        snapshot.rows()[32].status,
        BlockStatus::Skipped(SkipReason::DocumentBytes)
    );
    assert_eq!(
        snapshot.summary().participating_utf8,
        MAX_PARTICIPATING_UTF8 as u64
    );
    assert!(text(&model, snapshot.rows()[0].logical).contains(&body));
    // Preserve the real backend's error identity in this diagnostic, without changing core deps.
    let set = engine::default_syntaxes_for_test();
    let syntax = set.find_syntax_by_extension("rs").unwrap();
    let mut parser = syntect::parsing::ParseState::new(syntax);
    let mut op = root.begin(CancellationToken::new());
    let work = work::Work::new(&mut op);
    let mut monitor = work::Monitor(&work);
    let mut control = syntect::parsing::ExecutionControl::new(&mut monitor);
    let error = parser
        .parse_line_with_control(&line, set, &mut control)
        .unwrap_err();
    let syntect::parsing::ParsingError::Execution(syntect::parsing::ExecutionError::Regex {
        source,
        ..
    }) = error
    else {
        panic!("wrong error: {error:?}")
    };
    assert!(
        format!("{source:?}").contains("BacktrackLimitExceeded"),
        "{source:?}"
    );
}

#[test]
fn mid_block_cancel_and_cumulative_work_exhaustion_roll_back_and_stop_remaining_blocks() {
    let root = SourceBudget::preview();
    let model = model(&root, "```rs\n42\n```\n\n```rs\n43\n```\n", 1);
    for limit in [false, true] {
        let cancel = CancellationToken::new();
        let mut op = root.begin(cancel.clone());
        let mut calls = 0;
        let (buffers, summary) = build::build_with(&model, &mut op, |_, _, _, work, spans| {
            calls += 1;
            work.with_op(|op| {
                spans.push(
                    TokenSpan {
                        start_utf16: 0,
                        length_utf16: 1,
                        kind: SemanticKind::Number,
                    },
                    op,
                )
            })?;
            if limit {
                work.checkpoint(MAX_EXTERNAL_EVENTS as u32)?;
            } else {
                cancel.cancel();
                work.checkpoint(0)?;
            }
            panic!("terminal operation resumed")
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert!(
            matches!(
                summary.status,
                SnapshotStatus::Aborted(CoreError::Cancelled)
            ) && !limit
                || matches!(
                    summary.status,
                    SnapshotStatus::Aborted(CoreError::ResourceLimit {
                        kind: LimitKind::Work,
                        ..
                    })
                ) && limit
        );
        assert!(buffers.spans.values().is_empty());
        assert!(
            buffers
                .rows
                .values
                .iter()
                .all(|row| row.status == BlockStatus::Pending && row.spans.len == 0)
        );
        let mut body = root.begin(CancellationToken::new());
        body.checkpoint_now().unwrap();
    }
}

#[test]
fn work_budget_is_cumulative_and_zero_unit_monitor_checks_real_operation_clock() {
    let root = SourceBudget::preview();
    let mut op = root.begin(CancellationToken::new());
    let work = work::Work::new(&mut op);
    work.checkpoint(MAX_EXTERNAL_EVENTS as u32).unwrap();
    let expected = CoreError::ResourceLimit {
        kind: LimitKind::Work,
        requested: MAX_EXTERNAL_EVENTS + 1,
        limit: MAX_EXTERNAL_EVENTS,
    };
    assert_eq!(work.checkpoint(1), Err(expected));
    assert_eq!(work.checkpoint(0), Err(expected));
    assert_eq!(work.events(), MAX_EXTERNAL_EVENTS);
    let mut body = root.begin(CancellationToken::new());
    body.checkpoint_now().unwrap();
    let mut timed = root.begin(CancellationToken::new());
    timed.set_clock_samples_for_test(&[5_000_000_000]);
    let work = work::Work::new(&mut timed);
    assert!(matches!(
        work.checkpoint(0),
        Err(CoreError::DeadlineExceeded { .. })
    ));
    assert_eq!(work.events(), 0);
    body.checkpoint_now().unwrap();
}

#[test]
fn auxiliary_failure_after_table_allocation_is_typed_aborted_and_releases() {
    let root = SourceBudget::preview();
    let model = model(&root, "```rs\nlet n = 42;\n```\n", 1);
    let baseline = root.usage().auxiliary_capacity;
    let mut holder_op = root.begin(CancellationToken::new());
    let rows = MAX_BLOCKS * size_of::<HighlightBlock>();
    let held = holder_op
        .reserve_boundary(root.limits().auxiliary_capacity - baseline - rows as u64 - 1)
        .unwrap();
    let mut op = root.begin(CancellationToken::new());
    let snapshot = HighlightSnapshot::build(model, &mut op).unwrap();
    assert!(matches!(
        snapshot.summary().status,
        SnapshotStatus::Aborted(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        })
    ));
    assert_eq!(snapshot.rows()[0].status, BlockStatus::Pending);
    assert!(snapshot.spans().is_empty());
    drop(snapshot);
    drop(held);
    assert_eq!(root.usage().auxiliary_capacity, 0);
}

#[test]
fn wrong_root_and_precancel_are_errors_without_poisoning_body_operation() {
    let root = SourceBudget::preview();
    let model = model(&root, "```rs\n42\n```\n", 1);
    let foreign = SourceBudget::preview();
    let mut op = foreign.begin(CancellationToken::new());
    assert!(matches!(
        HighlightSnapshot::build(model.clone(), &mut op),
        Err(CoreError::WrongBudget)
    ));
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut op = root.begin(cancel);
    assert!(matches!(
        HighlightSnapshot::build(model.clone(), &mut op),
        Err(CoreError::Cancelled)
    ));
    let mut body = root.begin(CancellationToken::new());
    body.checkpoint_now().unwrap();
    assert_eq!(
        text(
            &model,
            model.ir().nodes()[snapshot(model.clone()).rows()[0].node.index()].logical
        ),
        "42\n"
    );
}

#[test]
fn real_g3_block_failure_discards_prior_line_tokens_and_allows_next_block() {
    use std::collections::HashMap;
    use syntect::parsing::syntax_definition::{Context, MatchOperation, MatchPattern, Pattern};
    use syntect::parsing::{Scope, SyntaxDefinition, SyntaxSetBuilder};
    let mut context = Context::new(false);
    for expression in [r"^(a+)+\1$", "."] {
        context.patterns.push(Pattern::Match(MatchPattern::new(
            false,
            expression.into(),
            vec![Scope::new("keyword.r4-test").unwrap()],
            None,
            MatchOperation::None,
            None,
        )));
    }
    let mut builder = SyntaxSetBuilder::new();
    builder.add(SyntaxDefinition {
        name: "synthetic fixed negative".into(),
        file_extensions: vec!["rs".into()],
        scope: Scope::new("source.r4-test").unwrap(),
        first_line_match: None,
        hidden: false,
        variables: HashMap::new(),
        contexts: HashMap::from([
            ("__start".into(), context.clone()),
            ("main".into(), context),
        ]),
    });
    let syntax = builder.build();
    let root = SourceBudget::preview();
    let model = model(
        &root,
        &format!("```rs\na\n{}!\n```\n\n```rs\naa\n```\n", "a".repeat(8190)),
        1,
    );
    let mut op = root.begin(CancellationToken::new());
    let (buffers, summary) =
        build::build_with(&model, &mut op, |_, code, expected, work, spans| {
            engine::tokenize_with_syntax(
                &syntax,
                &syntax.syntaxes()[0],
                code,
                expected,
                work,
                spans,
            )
        })
        .unwrap();
    assert_eq!(summary.failed_blocks, 1);
    assert_eq!(
        buffers.rows.values[0].status,
        BlockStatus::Failed(FailureReason::RegexSearch)
    );
    assert_eq!(buffers.rows.values[0].spans.len, 0);
    assert_eq!(buffers.rows.values[1].status, BlockStatus::Complete);
    assert_eq!(buffers.rows.values[1].spans.start, 0);
    assert_eq!(op.first_fault(), None);
}

#[test]
fn span_limit_rolls_back_whole_block_and_snapshot_last_owner_releases_cache() {
    let root = SourceBudget::preview();
    let model = model(&root, "```rs\n42\n```\n", 1);
    let mut op = root.begin(CancellationToken::new());
    let (buffers, summary) = build::build_with(&model, &mut op, |_, _, _, work, spans| {
        for _ in 0..=MAX_SPANS {
            work.checkpoint(1)?;
            if !work.with_op(|op| {
                spans.push(
                    TokenSpan {
                        start_utf16: 0,
                        length_utf16: 1,
                        kind: SemanticKind::Plain,
                    },
                    op,
                )
            })? {
                return Ok(engine::Outcome::SpanLimit);
            }
        }
        panic!("span limit not enforced")
    })
    .unwrap();
    assert_eq!(summary.skipped_blocks, 1);
    assert_eq!(
        buffers.rows.values[0].status,
        BlockStatus::Skipped(SkipReason::SpanCount)
    );
    assert!(buffers.spans.values().is_empty());
    assert!(root.usage().auxiliary_capacity > 0);
    drop(buffers);
    let snapshot = Arc::new(snapshot(model.clone()));
    let other = snapshot.clone();
    drop(model);
    drop(snapshot);
    assert!(root.usage().source_capacity > 0);
    assert!(root.usage().auxiliary_capacity > 0);
    drop(other);
    assert_eq!(root.usage().source_capacity, 0);
    assert_eq!(root.usage().auxiliary_capacity, 0);
}

#[test]
fn growth_counts_old_and_new_capacity_and_source_identity_stays_immutable() {
    let root = SourceBudget::preview();
    let mut op = root.begin(CancellationToken::new());
    let mut values = storage::GrowingRows::<u64>::new(1024);
    for value in 0..65 {
        values.push(value, &mut op).unwrap();
    }
    assert_eq!(values.values().len(), 65);
    assert!(root.usage().auxiliary_capacity_peak >= (64 + 128) * size_of::<u64>() as u64);
    assert_eq!(
        root.usage().auxiliary_capacity,
        128 * size_of::<u64>() as u64
    );
    drop(values);
    assert_eq!(root.usage().auxiliary_capacity, 0);
    let first = snapshot(model(&root, "```rs\n42\n```\n", 1));
    let second = snapshot(model(&root, "```rs\n\"text\"\n```\n", 2));
    assert_ne!(first.identity(), second.identity());
    assert_eq!(first.identity().source().revision(), 1);
    assert_eq!(second.identity().source().revision(), 2);
    assert!(
        first
            .spans()
            .iter()
            .any(|span| span.kind == SemanticKind::Number)
    );
    assert!(
        second
            .spans()
            .iter()
            .any(|span| span.kind == SemanticKind::String)
    );
}
