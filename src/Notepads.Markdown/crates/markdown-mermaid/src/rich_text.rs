// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Pure, synchronous whole-line text service. Spans share one shaping context;
//! a provider must not replace this contract with separately measured widths.

use crate::canonical::{CanonicalError, OutlineProvider, PathSink, RunMetrics, TextRun};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDirection {
    #[default]
    Auto,
    Ltr,
    Rtl,
}

/// UAX#9 paragraph P2/P3 first-strong selection, excluding isolate contents.
/// Static Unicode tables only; callers fund/charge the source scan beforehand.
pub fn resolve_direction(spans: &[TextRun<'_>], requested: TextDirection) -> TextDirection {
    if requested != TextDirection::Auto {
        return requested;
    }
    use unicode_bidi::{BidiClass, bidi_class};
    let mut isolates = 0usize;
    for span in spans {
        for c in span.text.chars() {
            match bidi_class(c) {
                BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => isolates += 1,
                BidiClass::PDI => isolates = isolates.saturating_sub(1),
                BidiClass::L if isolates == 0 => return TextDirection::Ltr,
                BidiClass::R | BidiClass::AL if isolates == 0 => return TextDirection::Rtl,
                _ => {}
            }
        }
    }
    TextDirection::Ltr
}

pub trait RichTextProvider: OutlineProvider {
    fn measure_spans(
        &self,
        spans: &[TextRun<'_>],
        direction: TextDirection,
    ) -> Result<RunMetrics, CanonicalError>;

    /// Outlines precisely the same single line, with the same format ranges and
    /// paragraph direction, at alphabetic baseline (0,0), y down.
    fn outline_spans(
        &self,
        spans: &[TextRun<'_>],
        direction: TextDirection,
        sink: &mut dyn PathSink,
    ) -> Result<(), CanonicalError>;
}
