//! Welcome to the syntect docs.
//!
//! Much more info about syntect is available on the [Github Page](https://github.com/trishume/syntect).
//!
//! May I suggest that you start by reading the `Readme.md` file in the main repo.
//! Once you're done with that you can look at the docs for [`parsing::SyntaxSet`]
//! and for the [`easy`] module.
//!
//! Almost everything in syntect is divided up into either the [`parsing`] module
//! for turning text into text annotated with scopes, and the [`highlighting`] module
//! for turning annotated text into styled/colored text.
//!
//! Some docs have example code but a good place to look is the `syncat` example as
//! well as the source code for the [`easy`] module in `easy.rs` as that shows how to
//! plug the various parts together for common use cases.
//!
//! [`parsing::SyntaxSet`]: parsing/struct.SyntaxSet.html
//! [`easy`]: easy/index.html
//! [`parsing`]: parsing/index.html
//! [`highlighting`]: highlighting/index.html

#![doc(html_root_url = "https://docs.rs/syntect/5.3.0")]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

#[cfg(any(feature = "dump-load", feature = "dump-create"))]
pub mod dumps;
#[cfg(feature = "parsing")]
pub mod easy;
#[cfg(feature = "html")]
mod escape;
pub mod highlighting;
#[cfg(feature = "html")]
pub mod html;
pub mod parsing;
pub mod util;
mod utils;

use std::io::Error as IoError;

#[cfg(feature = "plist-load")]
use crate::highlighting::{ParseThemeError, SettingsError};

/// An error enum for all things that can go wrong within syntect.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred while loading a syntax or theme
    #[error("Loading error: {0}")]
    LoadingError(#[from] LoadingError),
    /// An error occurred while parsing
    #[cfg(feature = "parsing")]
    #[error("Parsing error: {0}")]
    ParsingError(#[from] crate::parsing::ParsingError),
    /// Scope error
    #[error("Scope error: {0}")]
    ScopeError(#[from] crate::parsing::ScopeError),
    /// Formatting error
    #[error("Formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
    /// IO Error
    #[error("IO Error: {0}")]
    Io(#[from] IoError),
}

/// Common error type used by syntax and theme loading
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadingError {
    /// error finding all the files in a directory
    #[error("error finding all the files in a directory: {0}")]
    WalkDir(#[from] walkdir::Error),
    /// error reading a file
    #[error("error reading a file: {0}")]
    Io(#[from] IoError),
    /// a syntax file was invalid in some way
    #[cfg(all(feature = "yaml-load", feature = "parsing"))]
    #[error("{1}: {0}")]
    ParseSyntax(#[source] crate::parsing::ParseSyntaxError, String),
    /// a metadata file was invalid in some way
    #[cfg(feature = "metadata")]
    #[error("Failed to parse JSON")]
    ParseMetadata(#[from] serde_json::Error),
    /// a theme file was invalid in some way
    #[cfg(feature = "plist-load")]
    #[error("Invalid syntax theme")]
    ParseTheme(#[from] ParseThemeError),
    /// a theme's Plist syntax was invalid in some way
    #[cfg(feature = "plist-load")]
    #[error("Invalid syntax theme settings")]
    ReadSettings(#[from] SettingsError),
    /// A path given to a method was invalid.
    /// Possibly because it didn't reference a file or wasn't UTF-8.
    #[error("Invalid path")]
    BadPath,
}
