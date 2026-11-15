// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Complete fixed parsing policy identity; profiles never silently fall back.

use crate::error::CoreError;

/// Independently selected document extensions, separate from the base dialect.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Extensions(u32);

impl Extensions {
    pub const NONE: Self = Self(0);
    pub const MATH: Self = Self(1);
    pub const MERMAID: Self = Self(2);
    pub const HTML_BLOCKS: Self = Self(4);
    pub const HTML_INLINE: Self = Self(8);
    pub const HEADING_IDS: Self = Self(16);
    pub const FOOTNOTES: Self = Self(32);
    pub const CALLOUTS: Self = Self(64);
    pub const DESCRIPTION_LISTS: Self = Self(128);
    pub const FRONT_MATTER: Self = Self(0x100);
    pub const FRONT_MATTER_CODE: Self = Self(0x300);
    pub const FRONT_MATTER_HIDE: Self = Self(0x500);
    pub const HIGHLIGHT: Self = Self(0x800);
    pub const INLINE_SCRIPTS: Self = Self(0x1000);
    pub const EMOJI_SHORTCODES: Self = Self(0x2000);
    pub const TABLE_OF_CONTENTS: Self = Self(0x4000);
    pub const LOCAL_WIKI_LINKS: Self = Self(0x8000);
    pub const NOTE_EMBEDS: Self = Self(0x10000);
    pub const CITATIONS: Self = Self(0x20000);
    /// Default document extensions; front matter uses the table mode.
    pub const ALL: Self = Self(0x3f9ff);
    pub const SUPPORTED_BITS: u32 = 0x3ffff;

    pub const fn from_bits(bits: u32) -> Result<Self, CoreError> {
        let mode = bits & 0x600;
        if bits & !Self::SUPPORTED_BITS != 0
            || (mode != 0 && bits & Self::FRONT_MATTER.0 == 0)
            || mode == 0x600
        {
            Err(CoreError::UnsupportedConfiguration)
        } else {
            Ok(Self(bits))
        }
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, extension: Self) -> bool {
        self.0 & extension.0 == extension.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum Profile {
    C0,
    #[default]
    G3,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ParseConfig {
    profile: Profile,
    extensions: Extensions,
}

impl ParseConfig {
    pub const fn new(profile: Profile) -> Self {
        Self::with_extensions(profile, Extensions::NONE)
    }

    pub const fn with_extensions(profile: Profile, extensions: Extensions) -> Self {
        Self {
            profile,
            extensions,
        }
    }

    pub const fn profile(self) -> Profile {
        self.profile
    }

    pub const fn extensions(self) -> Extensions {
        self.extensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_masks_validate_independent_options_and_front_matter_modes() {
        for base in (0..=255).flat_map(|bits| (0..128).map(move |more| bits | (more << 11))) {
            for mode in [0, 0x100, 0x300, 0x500] {
                let bits = base | mode;
                let extensions = Extensions::from_bits(bits).unwrap();
                for profile in [Profile::C0, Profile::G3] {
                    let config = ParseConfig::with_extensions(profile, extensions);
                    assert_eq!(config.profile(), profile);
                    assert_eq!(config.extensions().bits(), bits);
                }
            }
        }
        for bit in [9, 10].into_iter().chain(18..32) {
            assert_eq!(
                Extensions::from_bits(1 << bit),
                Err(CoreError::UnsupportedConfiguration)
            );
        }
        for bits in [0x600, 0x700, 0x70f] {
            assert_eq!(
                Extensions::from_bits(bits),
                Err(CoreError::UnsupportedConfiguration)
            );
        }
    }
}
