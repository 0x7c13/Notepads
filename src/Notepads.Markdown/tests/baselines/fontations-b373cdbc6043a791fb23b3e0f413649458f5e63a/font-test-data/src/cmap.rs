//! cmap test data for scenarios not readily produced with ttx

use crate::{be_buffer, bebuffer::BeBuffer};

/// Contains two codepoint ranges, both [6, 64]. Surely you don't duplicate them?
pub fn repetitive_cmap4() -> BeBuffer {
    // <https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values>
    be_buffer! {
      4_u16,                      // uint16	format
      0_u16,                      // uint16	length, unused
      0_u16,                      // uint16	language, unused
      4_u16,                      // uint16	segCountX2, 2 * 2 segments
      0_u16,                      // uint16	searchRange, unused
      0_u16,                      // uint16	entrySelector, unused
      0_u16,                      // uint16	rangeShift, unused
      // segCount endCode entries
      64_u16,                    // uint16	endCode[0]
      64_u16,                    // uint16	endCode[1]

      0_u16,                      // uint16	reservedPad, unused

      // segCount startCode entries
      6_u16,                      // uint16	startCode[0]
      6_u16,                      // uint16	startCode[1]

      // segCount idDelta entries
      0_u16,                      // uint16	idDelta[0]
      0_u16,                      // uint16	idDelta[1]

      // segCount idRangeOffset entries
      0_u16,                      // uint16	idRangeOffset[0]
      0_u16                       // uint16	idRangeOffset[1]

      // no glyphIdArray entries
    }
}
