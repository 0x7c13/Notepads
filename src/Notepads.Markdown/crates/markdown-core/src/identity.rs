// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Owner-relative byte domains; detached spans carry no source authority.

use crate::config::ParseConfig;
use crate::error::CoreError;
use std::num::NonZeroU64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SourceKey {
    document: [u8; 16],
    revision: NonZeroU64,
}

impl SourceKey {
    pub fn new(document: [u8; 16], revision: u64) -> Result<Self, CoreError> {
        Ok(Self {
            document,
            revision: NonZeroU64::new(revision).ok_or(CoreError::InvalidRevision)?,
        })
    }

    pub fn document(self) -> [u8; 16] {
        self.document
    }

    pub fn revision(self) -> u64 {
        self.revision.get()
    }
}

macro_rules! byte_span {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name {
            start: u64,
            end: u64,
        }

        impl $name {
            pub fn new(start: u64, end: u64) -> Result<Self, CoreError> {
                if start > end {
                    return Err(CoreError::InvalidBoundary);
                }
                Ok(Self { start, end })
            }

            pub fn start(self) -> u64 {
                self.start
            }

            pub fn end(self) -> u64 {
                self.end
            }

            pub fn len(self) -> u64 {
                self.end - self.start
            }

            pub fn is_empty(self) -> bool {
                self.start == self.end
            }
        }
    };
}

byte_span!(SourceSpan);
byte_span!(ParseSpan);
byte_span!(LogicalSpan);
byte_span!(PayloadSpan);
byte_span!(LocalUtf8Span);

macro_rules! boundary {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name(u64);

        impl $name {
            /// The owner validates the domain and scalar boundary during queries.
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

boundary!(SourceBoundary);
boundary!(LogicalBoundary);
boundary!(PayloadBoundary);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ModelIdentity {
    source: SourceKey,
    generation: NonZeroU64,
    config: ParseConfig,
}

impl ModelIdentity {
    pub fn new(source: SourceKey, generation: u64, config: ParseConfig) -> Result<Self, CoreError> {
        Ok(Self {
            source,
            generation: NonZeroU64::new(generation).ok_or(CoreError::InvalidRevision)?,
            config,
        })
    }

    pub const fn source(self) -> SourceKey {
        self.source
    }

    pub const fn generation(self) -> u64 {
        self.generation.get()
    }

    pub const fn config(self) -> ParseConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_and_domains_are_checked() {
        assert_eq!(SourceKey::new([0; 16], 0), Err(CoreError::InvalidRevision));
        assert_eq!(
            SourceKey::new([0; 16], u64::MAX).unwrap().revision(),
            u64::MAX
        );
        assert_eq!(SourceSpan::new(2, 1), Err(CoreError::InvalidBoundary));
        assert_eq!(ParseSpan::new(0, u64::MAX).unwrap().len(), u64::MAX);
        assert!(SourceSpan::new(3, 3).unwrap().is_empty());
    }
}
