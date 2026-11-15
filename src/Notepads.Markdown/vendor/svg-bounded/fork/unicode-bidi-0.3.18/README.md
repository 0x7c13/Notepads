<!-- Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. -->

## Notepads controlled fork (2026)

This local Notepads fork requires the project Rust 1.99 toolchain. The optional `notepads-markdown-resources` feature provides fallible, request-funded bidi operations over the same algorithm used by the ordinary API. Current integration and validation scope are documented in the Notepads.Markdown module README. Original upstream source and licenses are retained below and alongside this file.

# unicode-bidi

This crate implements the [Unicode Bidirectional Algorithm][tr9] for display of
mixed right-to-left and left-to-right text. It is written in safe Rust,
compatible with the current stable release.

[Documentation](https://docs.rs/unicode-bidi)

[![CI](https://github.com/servo/unicode-bidi/actions/workflows/main.yml/badge.svg)](https://github.com/servo/unicode-bidi/actions)
[![AppVeyor](https://img.shields.io/appveyor/ci/servo/unicode-bidi/main.svg)](https://ci.appveyor.com/project/servo/unicode-bidi)

[tr9]: https://www.unicode.org/reports/tr9/
