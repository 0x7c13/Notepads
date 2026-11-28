// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Fixed, unmodified KaTeX font bytes. Faces only borrow the embedded asset table.

use ratex_font::{FontId, katex_ttf_glyph_char};
use ttf_parser::{Face, GlyphId};

pub const FONT_COUNT: usize = 20;
pub const FONT_BYTES: usize = 513_664;
pub const FONT_ASSET_SET: &str = "katex-ttf-20-v1";
pub const FONT_ASSET_MANIFEST_SHA256: &str =
    "3dea8514dcaa8cd0aeea60485d637a70c0fd8066924527613a763457a5c75750";
pub(crate) const UNKNOWN_FONT: u16 = u16::MAX;

/// A sealed view of the fixed assets; callers cannot supply arbitrary font bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct FontAssets {
    _sealed: (),
}

impl FontAssets {
    pub const fn new() -> Self {
        Self { _sealed: () }
    }

    pub const fn asset_set(&self) -> &'static str {
        FONT_ASSET_SET
    }

    pub const fn len(&self) -> usize {
        FONT_COUNT
    }

    pub const fn is_empty(&self) -> bool {
        false
    }

    pub const fn byte_len(&self) -> usize {
        FONT_BYTES
    }

    pub(crate) fn face(&self, name: &str) -> Result<FontFace, FontLookupError> {
        let (index, asset) = ASSETS
            .iter()
            .enumerate()
            .find(|(_, asset)| asset.name == name)
            .ok_or(FontLookupError::UnknownFont)?;
        let font = index as u16;
        let face =
            Face::parse(asset.bytes, 0).map_err(|_| FontLookupError::InvalidFont { font })?;
        if face.units_per_em() == 0 {
            return Err(FontLookupError::InvalidFont { font });
        }
        Ok(FontFace {
            font,
            font_id: FontId::parse(asset.name),
            face,
        })
    }

    #[cfg(test)]
    pub(crate) fn name(&self, index: usize) -> Option<&'static str> {
        ASSETS.get(index).map(|asset| asset.name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontLookupError {
    UnknownFont,
    InvalidFont { font: u16 },
    InvalidCodepoint { font: u16, codepoint: u32 },
    MissingGlyph { font: u16, codepoint: u32 },
}

pub(crate) struct FontFace {
    font: u16,
    font_id: Option<FontId>,
    face: Face<'static>,
}

impl FontFace {
    pub(crate) fn index(&self) -> u16 {
        self.font
    }

    pub(crate) fn face(&self) -> &Face<'static> {
        &self.face
    }

    pub(crate) fn glyph(&self, codepoint: u32) -> Result<GlyphId, FontLookupError> {
        let original = char::from_u32(codepoint).ok_or(FontLookupError::InvalidCodepoint {
            font: self.font,
            codepoint,
        })?;
        // Caligraphic-Bold is retained with the original 20 assets, but has no
        // upstream FontId/metrics entry. It receives no invented metric mapping.
        let mapped = self
            .font_id
            .map_or(original, |font| katex_ttf_glyph_char(font, codepoint));
        self.face
            .glyph_index(mapped)
            .filter(|glyph| glyph.0 != 0)
            .ok_or(FontLookupError::MissingGlyph {
                font: self.font,
                codepoint,
            })
    }
}

struct FontAsset {
    name: &'static str,
    bytes: &'static [u8],
}

macro_rules! asset {
    ($name:literal, $file:literal) => {
        FontAsset {
            name: $name,
            bytes: include_bytes!(concat!("../assets/fonts/", $file)),
        }
    };
}

static ASSETS: [FontAsset; FONT_COUNT] = [
    asset!("AMS-Regular", "KaTeX_AMS-Regular.ttf"),
    asset!("Caligraphic-Bold", "KaTeX_Caligraphic-Bold.ttf"),
    asset!("Caligraphic-Regular", "KaTeX_Caligraphic-Regular.ttf"),
    asset!("Fraktur-Bold", "KaTeX_Fraktur-Bold.ttf"),
    asset!("Fraktur-Regular", "KaTeX_Fraktur-Regular.ttf"),
    asset!("Main-Bold", "KaTeX_Main-Bold.ttf"),
    asset!("Main-BoldItalic", "KaTeX_Main-BoldItalic.ttf"),
    asset!("Main-Italic", "KaTeX_Main-Italic.ttf"),
    asset!("Main-Regular", "KaTeX_Main-Regular.ttf"),
    asset!("Math-BoldItalic", "KaTeX_Math-BoldItalic.ttf"),
    asset!("Math-Italic", "KaTeX_Math-Italic.ttf"),
    asset!("SansSerif-Bold", "KaTeX_SansSerif-Bold.ttf"),
    asset!("SansSerif-Italic", "KaTeX_SansSerif-Italic.ttf"),
    asset!("SansSerif-Regular", "KaTeX_SansSerif-Regular.ttf"),
    asset!("Script-Regular", "KaTeX_Script-Regular.ttf"),
    asset!("Size1-Regular", "KaTeX_Size1-Regular.ttf"),
    asset!("Size2-Regular", "KaTeX_Size2-Regular.ttf"),
    asset!("Size3-Regular", "KaTeX_Size3-Regular.ttf"),
    asset!("Size4-Regular", "KaTeX_Size4-Regular.ttf"),
    asset!("Typewriter-Regular", "KaTeX_Typewriter-Regular.ttf"),
];

const _: () = {
    let mut index = 0;
    let mut total = 0;
    while index < ASSETS.len() {
        total += ASSETS[index].bytes.len();
        index += 1;
    }
    assert!(total == FONT_BYTES);
    assert!(total <= 1_048_576);
};
