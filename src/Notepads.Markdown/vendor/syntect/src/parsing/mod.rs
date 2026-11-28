//! Everything about parsing text into text annotated with scopes.
//!
//! The most important struct here is [`SyntaxSet`], check out the docs for that.
//!
//! [`SyntaxSet`]: struct.SyntaxSet.html

// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#[cfg(feature = "parsing")]
mod execution;
#[cfg(feature = "parsing")]
pub use self::execution::*;

#[cfg(feature = "metadata")]
pub mod metadata;
#[cfg(feature = "parsing")]
mod parser;
#[cfg(feature = "parsing")]
pub mod syntax_definition;
#[cfg(feature = "parsing")]
mod syntax_set;
#[cfg(all(feature = "parsing", feature = "yaml-load"))]
mod yaml_load;

#[cfg(any(feature = "parsing", feature = "yaml-load", feature = "metadata"))]
mod regex;
mod scope;

#[cfg(feature = "metadata")]
pub use self::metadata::*;
#[cfg(feature = "parsing")]
pub use self::parser::*;
#[cfg(feature = "parsing")]
pub use self::syntax_definition::SyntaxDefinition;
#[cfg(feature = "parsing")]
pub use self::syntax_set::*;
#[cfg(all(feature = "parsing", feature = "yaml-load"))]
pub use self::yaml_load::*;

#[cfg(any(feature = "parsing", feature = "yaml-load", feature = "metadata"))]
pub use self::regex::*;

pub use self::scope::*;
