// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! One bounded size/fill epoch, owned typed paths, fixed coordinate normalization.
use crate::memory::Context;
use crate::{
    abi::{self, Failure, Result},
    memory::{AdmittedSlice, RetainedPool, failure_from_resource},
};
use markdown_svg_budget::{BudgetVec, ResourceKind, ResourcesRef};
use std::sync::Arc;

struct ReceiptGuard {
    callbacks: Context,
    generation: u64,
}
impl ReceiptGuard {
    fn retire(mut self) -> Result<()> {
        let generation = core::mem::take(&mut self.generation);
        // SAFETY: exact live epoch from prepare_geometry; no Native pointer stored.
        self.callbacks.native_success(unsafe {
            self.callbacks.retire_geometry.unwrap()(self.callbacks.session, generation)
        })
    }
}
impl Drop for ReceiptGuard {
    fn drop(&mut self) {
        if self.generation != 0 {
            // Native first-fault/ledger diagnostics reject failed retirement at coarse
            // exit. This drop never publishes output and never retries a retired epoch.
            let hr = unsafe {
                self.callbacks.retire_geometry.unwrap()(self.callbacks.session, self.generation)
            };
            let _ = self.callbacks.native_success(hr);
            self.generation = 0;
        }
    }
}
pub struct StoredPath {
    pub key: abi::GlyphKey,
    pub path: Option<Arc<tiny_skia::Path>>,
    pub fill_rule: u32,
}
pub struct OwnedGeometry {
    pub paths: BudgetVec<StoredPath>,
    _lifetime: crate::memory::OwnerWitness,
}
impl OwnedGeometry {
    pub fn fetch(
        callbacks: &Context,
        resources: &ResourcesRef,
        retained: &Arc<RetainedPool>,
        keys: &[abi::GlyphKey],
    ) -> Result<Self> {
        let candidate = {
            #[cfg(feature = "quality-observation")]
            {
                Self::fetch_inner(callbacks, resources, retained, keys, None)
            }
            #[cfg(not(feature = "quality-observation"))]
            {
                Self::fetch_inner(callbacks, resources, retained, keys)
            }
        };
        // Native receipt/POD buffers have retired inside fetch_inner. Their
        // release failure must stop before the next Native plan consumes us.
        resources.checkpoint(0).map_err(failure_from_resource)?;
        candidate
    }
    #[cfg(feature = "quality-observation")]
    pub(crate) fn fetch_observed(
        callbacks: &Context,
        resources: &ResourcesRef,
        retained: &Arc<RetainedPool>,
        keys: &[abi::GlyphKey],
        observation: &crate::observation::ObservationSink,
    ) -> Result<Self> {
        let candidate = Self::fetch_inner(callbacks, resources, retained, keys, Some(observation));
        resources.checkpoint(0).map_err(failure_from_resource)?;
        candidate
    }
    fn fetch_inner(
        callbacks: &Context,
        resources: &ResourcesRef,
        retained: &Arc<RetainedPool>,
        keys: &[abi::GlyphKey],
        #[cfg(feature = "quality-observation")] observation: Option<
            &crate::observation::ObservationSink,
        >,
    ) -> Result<Self> {
        if keys.is_empty() || keys.len() > 4096 {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let mut receipt = abi::GeometryReceipt {
            size: size_of::<abi::GeometryReceipt>() as u32,
            version: abi::ABI_VERSION,
            ..Default::default()
        };
        // SAFETY: bounded borrowed key batch and initialized receipt output.
        callbacks.native_success(unsafe {
            callbacks.prepare_geometry.unwrap()(
                callbacks.session,
                keys.as_ptr(),
                keys.len() as u32,
                &mut receipt,
            )
        })?;
        if receipt.generation == 0 {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let guard = ReceiptGuard {
            callbacks: callbacks.clone(),
            generation: receipt.generation,
        };
        if receipt.size as usize != size_of::<abi::GeometryReceipt>()
            || receipt.version != abi::ABI_VERSION
            || receipt.glyph_count as usize != keys.len()
            || receipt.command_count > 4_194_304
        {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let mut paths =
            AdmittedSlice::<abi::GlyphPath>::zeroed(callbacks, abi::GEOMETRY, keys.len())?;
        let mut commands = AdmittedSlice::<abi::Command>::zeroed(
            callbacks,
            abi::GEOMETRY,
            receipt.command_count as usize,
        )?;
        let mut paths_written = 0;
        let mut commands_written = 0;
        // SAFETY: exact admitted final typed POD capacities for this single epoch.
        callbacks.native_success(unsafe {
            callbacks.fill_geometry.unwrap()(
                callbacks.session,
                receipt.generation,
                paths.as_mut_ptr(),
                receipt.glyph_count,
                commands.as_mut_ptr(),
                receipt.command_count,
                &mut paths_written,
                &mut commands_written,
            )
        })?;
        if paths_written != receipt.glyph_count || commands_written != receipt.command_count {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        guard.retire()?; // Native capacities retire only after the complete Rust copy.
        let mut result = BudgetVec::new(resources.clone(), ResourceKind::Geometry);
        result
            .try_reserve_exact(keys.len())
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        let mut covered = 0;
        for (index, path) in paths.as_slice().iter().enumerate() {
            let start = path.command_start as usize;
            let end = start
                .checked_add(path.command_count as usize)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            if path.key_index as usize != index
                || start != covered
                || end > commands.len()
                || path.fill_rule > 1
            {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let key = keys[index];
            if key.orientation > 1 || key.reserved != 0 {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let verb_capacity = path.command_count as usize;
            let point_capacity = verb_capacity
                .checked_mul(3)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
            let mut builder = tiny_skia::PathBuilder::try_with_capacity(
                resources,
                ResourceKind::Geometry,
                verb_capacity,
                point_capacity,
            )
            .map_err(failure_from_resource)?;
            let mut open = false;
            let map = |x: f32, y: f32| -> Result<(f32, f32)> {
                if !x.is_finite() || !y.is_finite() {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
                // Approved fixed adapter. Never tune it after seeing actual raster.
                Ok(if key.orientation == 0 {
                    (x, -y)
                } else {
                    (-y, -x)
                })
            };
            for (offset, command) in commands.as_slice()[start..end].iter().enumerate() {
                if offset % 256 == 0 {
                    callbacks.checkpoint(256)?;
                }
                let (x1, y1) = map(command.x1, command.y1)?;
                let (x2, y2) = map(command.x2, command.y2)?;
                let (x3, y3) = map(command.x3, command.y3)?;
                match command.kind {
                    0 => {
                        builder.try_move_to(x1, y1).map_err(failure_from_resource)?;
                        open = true;
                    }
                    1 if open => builder.try_line_to(x1, y1).map_err(failure_from_resource)?,
                    2 if open => builder
                        .try_cubic_to(x1, y1, x2, y2, x3, y3)
                        .map_err(failure_from_resource)?,
                    3 if open => {
                        builder.try_close().map_err(failure_from_resource)?;
                        open = false;
                    }
                    _ => return Err(callbacks.record_failure(Failure::Contract)),
                }
            }
            if open {
                return Err(callbacks.record_failure(Failure::Contract));
            }
            let owned = match builder.try_finish().map_err(failure_from_resource)? {
                Some(path) => Some(retained.share(path)?),
                None => None,
            };
            result
                .try_push(StoredPath {
                    key,
                    path: owned,
                    fill_rule: path.fill_rule,
                })
                .map_err(failure_from_resource)?;
            covered = end;
        }
        if covered != commands.len() {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        #[cfg(feature = "quality-observation")]
        if let Some(observation) = observation.filter(|sink| sink.is_enabled()) {
            use std::fmt::Write;
            // These are still the actual Rust-owned fill outputs. The Native
            // borrow has already retired and every path/verb was validated.
            observation.record(|trace| {
                write!(trace, "{{\"kind\":\"actualNativeGeometryBatch\",\"generation\":{},\"nativeEpochRetired\":true,\"validated\":true,\"pathsWritten\":{paths_written},\"commandsWritten\":{commands_written},\"keys\":[", receipt.generation).map_err(|_| Failure::Contract)?;
                for (index, key) in keys.iter().enumerate() {
                    if index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "{{\"faceTicket\":{},\"glyph\":{},\"orientation\":{},\"reserved\":{}}}", key.face_ticket, key.glyph_id, key.orientation, key.reserved).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"typedPaths\":[").map_err(|_| Failure::Contract)?;
                for (index, path) in paths.as_slice().iter().enumerate() {
                    if index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "{{\"keyIndex\":{},\"commandStart\":{},\"commandCount\":{},\"fillRule\":{}}}", path.key_index, path.command_start, path.command_count, path.fill_rule).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"typedCommandsNativeYDown\":[").map_err(|_| Failure::Contract)?;
                for (index, command) in commands.as_slice().iter().enumerate() {
                    if index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "[{},{},{},{},{},{},{}]", command.kind, command.x1, command.y1, command.x2, command.y2, command.x3, command.y3).map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"nativeCommandKind\":\"Move0-Line1-Cubic2-Close3\",\"normalizedPaths\":[").map_err(|_| Failure::Contract)?;
                for (index, path) in result.iter().enumerate() {
                    if index != 0 { trace.write_char(',').map_err(|_| Failure::Contract)?; }
                    write!(trace, "{{\"keyIndex\":{index},\"faceTicket\":{},\"glyph\":{},\"orientation\":{},\"fillRule\":{},\"commandsLocalYUp\":", path.key.face_ticket, path.key.glyph_id, path.key.orientation, path.fill_rule).map_err(|_| Failure::Contract)?;
                    trace.path(path.path.as_deref()).map_err(|_| Failure::Contract)?;
                    trace.write_char('}').map_err(|_| Failure::Contract)?;
                }
                trace.write_str("],\"normalizedCommandKind\":\"Move0-Line1-Quad2-Cubic3-Close4\"}\n").map_err(|_| Failure::Contract)
            })?;
        }
        // POD copies drop here; the typed owner keeps its admitted storage alive.
        Ok(Self {
            paths: result,
            _lifetime: crate::memory::OwnerWitness::new(
                callbacks,
                crate::memory::OwnerKind::Geometry,
            ),
        })
    }
}
