#[cfg(feature = "diagram-class")]
mod class;
#[cfg(feature = "diagram-cynefin")]
mod cynefin;
mod detect;
#[cfg(feature = "diagram-er")]
mod er;
#[cfg(feature = "diagram-flowchart")]
mod flowchart;
#[cfg(feature = "diagram-info")]
mod info;
mod misc;
#[cfg(feature = "diagram-pie")]
mod pie;
#[cfg(feature = "diagram-railroad")]
mod railroad;
#[cfg(feature = "all-diagrams")]
mod registry;
#[cfg(feature = "diagram-sequence")]
mod sequence;
#[cfg(feature = "diagram-state")]
mod state;

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
mod flowchart_shared;
#[cfg(feature = "diagram-swimlane")]
mod swimlane;
