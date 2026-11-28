// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

/*!
`usvg` (micro SVG) is an [SVG] parser that tries to solve most of SVG complexity.

SVG is notoriously hard to parse. `usvg` presents a layer between an XML library and
a potential SVG rendering library. It will parse an input SVG into a strongly-typed tree structure
were all the elements, attributes, references and other SVG features are already resolved
and presented in the simplest way possible.
So a caller doesn't have to worry about most of the issues related to SVG parsing
and can focus just on the rendering part.

## Features

- All supported attributes are resolved.
  No need to worry about inheritable, implicit and default attributes
- CSS will be applied
- Only simple paths
  - Basic shapes (like `rect` and `circle`) will be converted into paths
  - Paths contain only absolute *MoveTo*, *LineTo*, *QuadTo*, *CurveTo* and *ClosePath* segments.
    ArcTo, implicit and relative segments will be converted
- `use` will be resolved and replaced with the reference content
- Nested `svg` will be resolved
- Invalid, malformed elements will be removed
- Relative length units (mm, em, etc.) will be converted into pixels/points
- External images will be loaded
- Internal, base64 images will be decoded
- All references (like `#elem` and `url(#elem)`) will be resolved
- `switch` will be resolved
- Text elements, which are probably the hardest part of SVG, will be completely resolved.
  This includes all the attributes resolving, whitespaces preprocessing (`xml:space`),
  text chunks and spans resolving
- Markers will be converted into regular elements. No need to place them manually
- All filters are supported. Including filter functions, like `filter="contrast(50%)"`
- Recursive elements will be detected and removed
- `objectBoundingBox` will be replaced with `userSpaceOnUse`

## Limitations

- Unsupported SVG features will be ignored
- CSS support is minimal
- Only [static](http://www.w3.org/TR/SVG11/feature#SVG-static) SVG features,
  e.g. no `a`, `view`, `cursor`, `script`, no events and no animations

[SVG]: https://en.wikipedia.org/wiki/Scalable_Vector_Graphics
*/

#![cfg_attr(not(test), forbid(unsafe_code))]
#![cfg_attr(test, deny(unsafe_code))]
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![warn(missing_copy_implementations)]

mod encoded;
mod processing;
mod scene;
pub use encoded::EncodedImageData;
pub use processing::{ProcessingContext, ProcessingError, ProcessingFailure};
mod parser;
#[cfg(feature = "text")]
mod text;
mod tree;
#[cfg(feature = "writer")]
mod writer;

pub use parser::*;
#[cfg(feature = "text")]
pub use text::*;
pub use tree::*;

pub use roxmltree;

mod geometry;
#[cfg(all(test, feature = "text"))]
#[allow(unsafe_code)] // Test-only System allocator observer; production remains forbid.
mod geometry_test_support;
#[cfg(feature = "text")]
mod outline_cache;

#[cfg(feature = "text")]
pub use fontdb;

#[cfg(feature = "writer")]
pub use writer::WriteOptions;
#[cfg(feature = "writer")]
pub use xmlwriter::Indent;

#[cfg(feature = "text")]
mod font_replay;
#[cfg(feature = "text")]
pub use font_replay::*;

#[cfg(feature = "text")]
mod glyph_coverage;
#[cfg(feature = "text")]
pub use glyph_coverage::{GlyphFillSource, GlyphOutlineKind, PlacedGlyphSource};
