// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Same-job borrowed observations. None allocates no trace or nominal table.
use crate::abi::{self, Result};
#[cfg(feature = "quality-observation")]
use crate::{abi::Failure, quality_trace::TraceWriter};
use std::sync::Arc;

#[cfg(feature = "quality-observation")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QualityTraceInfo {
    pub bytes: u64,
    pub capacity: u64,
    pub events: u64,
    /// Bit 0: installed; bit 1: a complete event did not fit.
    pub flags: u32,
    /// 0: none; 1: capacity; 2: format/contract; 3: interrupted storage; 4: work/cancel.
    pub error: u32,
}

pub(crate) trait Observation: Send + Sync {
    fn shaped(&self, _draft: u64, _generation: u64, _text: &usvg::ReplayText) -> Result<()> {
        Ok(())
    }
    fn metrics(
        &self,
        _face: abi::Face,
        _orientation: u32,
        _value: usvg::ReplayMetricsObservation<'_>,
    ) -> Result<()> {
        Ok(())
    }
    fn layout_metrics(
        &self,
        _face: abi::Face,
        _orientation: u32,
        _value: usvg::ReplayLayoutMetricsObservation,
    ) -> Result<()> {
        Ok(())
    }
    fn outline(
        &self,
        _face: abi::Face,
        _orientation: u32,
        _value: usvg::ReplayOutlineObservation<'_>,
    ) -> Result<()> {
        Ok(())
    }
    fn geometry(
        &self,
        _face: abi::Face,
        _orientation: u32,
        _value: usvg::ReplayGeometryObservation<'_>,
    ) -> Result<()> {
        Ok(())
    }
    #[cfg(feature = "quality-observation")]
    fn record(&self, _action: &mut dyn FnMut(&mut TraceWriter) -> Result<()>) -> Result<()> {
        Err(Failure::Contract)
    }
    #[cfg(feature = "quality-observation")]
    fn info(&self) -> Result<QualityTraceInfo> {
        Err(Failure::Contract)
    }
    #[cfg(feature = "quality-observation")]
    fn copy(&self, _destination: &mut [u8]) -> Result<usize> {
        Err(Failure::Contract)
    }
}

#[derive(Default)]
pub(crate) struct ObservationSink(Option<Arc<dyn Observation>>);
impl ObservationSink {
    #[cfg(feature = "quality-observation")]
    pub(crate) fn is_enabled(&self) -> bool {
        self.0.is_some()
    }
    pub(crate) fn shaped(
        &self,
        draft: u64,
        generation: u64,
        text: &usvg::ReplayText,
    ) -> Result<()> {
        self.0
            .as_ref()
            .map_or(Ok(()), |sink| sink.shaped(draft, generation, text))
    }
    pub(crate) fn metrics(
        &self,
        face: abi::Face,
        orientation: u32,
        value: usvg::ReplayMetricsObservation<'_>,
    ) -> Result<()> {
        self.0
            .as_ref()
            .map_or(Ok(()), |sink| sink.metrics(face, orientation, value))
    }
    pub(crate) fn layout_metrics(
        &self,
        face: abi::Face,
        orientation: u32,
        value: usvg::ReplayLayoutMetricsObservation,
    ) -> Result<()> {
        self.0
            .as_ref()
            .map_or(Ok(()), |sink| sink.layout_metrics(face, orientation, value))
    }
    pub(crate) fn outline(
        &self,
        face: abi::Face,
        orientation: u32,
        value: usvg::ReplayOutlineObservation<'_>,
    ) -> Result<()> {
        self.0
            .as_ref()
            .map_or(Ok(()), |sink| sink.outline(face, orientation, value))
    }
    pub(crate) fn geometry(
        &self,
        face: abi::Face,
        orientation: u32,
        value: usvg::ReplayGeometryObservation<'_>,
    ) -> Result<()> {
        self.0
            .as_ref()
            .map_or(Ok(()), |sink| sink.geometry(face, orientation, value))
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn install(&mut self, sink: Arc<dyn Observation>) -> Result<()> {
        if self.0.is_some() {
            return Err(Failure::Contract);
        }
        self.0 = Some(sink);
        Ok(())
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn record(&self, action: impl FnOnce(&mut TraceWriter) -> Result<()>) -> Result<()> {
        let Some(sink) = &self.0 else { return Ok(()) };
        let mut action = Some(action);
        sink.record(&mut |writer| action.take().ok_or(Failure::Contract)?(writer))
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn info(&self) -> Result<QualityTraceInfo> {
        self.0.as_ref().ok_or(Failure::Contract)?.info()
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn copy(&self, destination: &mut [u8]) -> Result<usize> {
        self.0.as_ref().ok_or(Failure::Contract)?.copy(destination)
    }
}
