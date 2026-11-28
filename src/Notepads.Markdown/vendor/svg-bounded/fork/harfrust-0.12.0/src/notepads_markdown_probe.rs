// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Read-only properties for the private Notepads Markdown replay adapter.
//! This module changes no Unicode table or shaping behavior.

use crate::hb::unicode::{CharExt, GeneralCategory};

/// The fixed upstream suppression policy, not the Unicode DICP property.
pub const SHAPING_SUPPRESSION_PROFILE: &str = "harfrust-0.12.0-is-default-ignorable";

/// Reads General_Category=Me from the original Unicode 17 UCD table.
#[must_use]
pub fn is_enclosing_mark(value: char) -> bool {
    (value as u32).general_category() == GeneralCategory::ENCLOSING_MARK
}

/// Reads the unchanged HarfRust 0.12.0 suppression candidate policy.
///
/// A true value alone proves neither zero advance nor absence of ink.
/// The actual checked source/HB/representation results must establish those.
#[must_use]
pub fn shaping_suppression_candidate(value: char) -> bool {
    (value as u32).is_default_ignorable()
}
