// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! GFM tagfilter runs on grammar-owned raw HTML, before safe projection.

use super::fragment::{Fragment, MAX_FRAGMENT_BYTES};
use comrak::native::gfm_tagfilter_matches;
use std::borrow::Cow;
use std::ops::Range;

const ENTITY: &str = "&lt;";
const GROWTH: usize = ENTITY.len() - 1;

pub(super) struct Filtered<'a> {
    pub text: Cow<'a, str>,
    // Endpoints immediately after each inserted entity in filtered coordinates.
    ends: Vec<usize>,
}

impl<'a> Filtered<'a> {
    pub fn new(raw: &'a str, enabled: bool) -> Option<Self> {
        let mut ends = Vec::new();
        if enabled {
            for (at, ch) in raw.bytes().enumerate() {
                if ch == b'<' && gfm_tagfilter_matches(&raw.as_bytes()[at..]) {
                    let new_len = raw.len() + (ends.len() + 1) * GROWTH;
                    if new_len > MAX_FRAGMENT_BYTES {
                        return None;
                    }
                    ends.push(at + ends.len() * GROWTH + ENTITY.len());
                }
            }
        }
        if ends.is_empty() {
            return Some(Self {
                text: Cow::Borrowed(raw),
                ends,
            });
        }
        let mut text = String::with_capacity(raw.len() + ends.len() * GROWTH);
        let mut last = 0;
        for (index, end) in ends.iter().copied().enumerate() {
            let at = end - ENTITY.len() - index * GROWTH;
            text.push_str(&raw[last..at]);
            text.push_str(ENTITY);
            last = at + 1;
        }
        text.push_str(&raw[last..]);
        Some(Self {
            text: Cow::Owned(text),
            ends,
        })
    }

    pub fn changed(&self) -> bool {
        !self.ends.is_empty()
    }

    fn original(&self, at: usize) -> Option<usize> {
        let inserted = self.ends.partition_point(|end| *end <= at);
        if self
            .ends
            .get(inserted)
            .is_some_and(|end| at > *end - ENTITY.len())
        {
            return None;
        }
        Some(at - inserted * GROWTH)
    }

    fn remap(&self, range: &mut Range<usize>) -> Option<()> {
        *range = self.original(range.start)?..self.original(range.end)?;
        Some(())
    }

    pub fn restore(&self, fragment: &mut Fragment) -> Option<()> {
        if !self.changed() {
            return Some(());
        }
        for part in &mut fragment.parts {
            self.remap(&mut part.source)?;
        }
        for destination in &mut fragment.destinations {
            let len = destination.source.len();
            self.remap(&mut destination.source)?;
            destination.exact &= destination.source.len() == len;
        }
        for picture in &mut fragment.pictures {
            for variant in picture.variants() {
                self.remap(&mut variant.source)?;
            }
        }
        for control in &mut fragment.static_checkboxes {
            self.remap(&mut control.source)?;
        }
        if let Some(table) = &mut fragment.html_table {
            self.remap(&mut table.source)?;
            for cell in &mut table.cells {
                self.remap(&mut cell.source)?;
            }
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_case_insensitive_tag_boundaries_and_preserves_input_coordinates() {
        let raw =
            "甲<ScRiPt title='<iframe>'>β</sCrIpT><title/><title-long><title/ bogus>&lt;style>";
        let filtered = Filtered::new(raw, true).unwrap();
        assert_eq!(
            filtered.text,
            "甲&lt;ScRiPt title='&lt;iframe>'>β&lt;/sCrIpT>&lt;title/><title-long><title/ bogus>&lt;style>"
        );
        for (index, end) in filtered.ends.iter().copied().enumerate() {
            let start = end - ENTITY.len();
            assert_eq!(filtered.original(start), Some(start - index * GROWTH));
            assert_eq!(
                filtered.original(end),
                filtered.original(start).map(|at| at + 1)
            );
            assert_eq!(filtered.original(start + 1), None);
        }
        assert!(matches!(
            Filtered::new(raw, false).unwrap().text,
            Cow::Borrowed(_)
        ));
        assert!(!gfm_tagfilter_matches(b"<title\x0b>"));
        assert!(!gfm_tagfilter_matches(b"<title\x0c>"));
        for name in [
            "title",
            "textarea",
            "style",
            "xmp",
            "iframe",
            "noembed",
            "noframes",
            "script",
            "plaintext",
        ] {
            assert!(gfm_tagfilter_matches(format!("</{name} >").as_bytes()));
            assert!(!gfm_tagfilter_matches(format!("<{name}").as_bytes()));
        }
    }
}
