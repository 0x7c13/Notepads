// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! The direct native compiler writes final rows before independent acceptance.

pub(crate) use direct::build;
pub(crate) use direct::build_with_projection;
pub(crate) use direct::projection::ProjectionFacts;
#[cfg(test)]
pub(crate) use direct::projection::projection_facts;
pub(crate) use direct::projection::{ProjectionBuild, build_or_project};

#[cfg(test)]
mod tests;

mod options;
mod prepared;

#[cfg(test)]
mod direct_composition;

mod direct;
