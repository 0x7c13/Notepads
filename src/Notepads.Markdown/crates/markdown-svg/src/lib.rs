// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Platform independent, one-shot SVG jobs using the bounded production engine.
//! A rendered candidate requires successful worker retirement before publication.

pub mod abi;
mod adapter;
pub mod coverage;
mod draft;
mod family;
mod font;
mod geometry;
mod grapheme;
mod grapheme_roles;
mod job;
mod logical;
mod memory;
mod nominal;
mod observation;
#[cfg(feature = "quality-observation")]
mod quality_trace;
mod resource_backend;
mod search_work;
mod shape;

#[cfg(test)]
mod colr_foreground_tests;
#[cfg(test)]
mod coverage_native_tests;
#[cfg(test)]
mod coverage_paint_tests;
#[cfg(test)]
mod synthetic_fill_tests;

pub use job::{
    BootstrapMetadata, CancelHandle, CleanupStatus, Identity, JobError, JobState, JobStats,
    NaturalSize, OwnedJob, RasterPlan, Rendered, Retirement, Target, bootstrap_metadata,
};
pub use memory::Host;
#[cfg(feature = "quality-observation")]
pub use observation::QualityTraceInfo;

#[cfg(test)]
mod xml_namespace_tests;
