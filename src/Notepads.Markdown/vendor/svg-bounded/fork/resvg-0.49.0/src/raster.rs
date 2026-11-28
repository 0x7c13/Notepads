// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Internal raster ownership. Controlled requests never hand storage to an
//! unpaired Vec/Pixmap; compatibility entry points retain their old storage.

use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, PaidShared, ResourceKind, ResourcesRef,
};

#[derive(Debug)]
enum Storage {
    Paid(BudgetVec<u8>),
    Compatibility(tiny_skia::Pixmap),
}

#[derive(Clone, Debug)]
pub(crate) enum SharedRaster {
    Paid(PaidShared<Raster>),
    Compatibility(std::rc::Rc<Raster>),
}

impl SharedRaster {
    pub(crate) fn try_new(value: Raster) -> Result<Self, Error> {
        match value.resources().cloned() {
            Some(resources) => {
                PaidShared::try_new(&resources, ResourceKind::Scratch, value).map(Self::Paid)
            }
            None => Ok(Self::Compatibility(std::rc::Rc::new(value))),
        }
    }
    pub(crate) fn as_ref(&self) -> &Raster {
        match self {
            Self::Paid(v) => v.as_ref(),
            Self::Compatibility(v) => v.as_ref(),
        }
    }
    pub(crate) fn take(self) -> Result<Raster, Error> {
        match self {
            Self::Paid(v) => match v.try_unwrap() {
                Ok(v) => Ok(v),
                Err(v) => v.as_ref().try_clone(),
            },
            Self::Compatibility(v) => match std::rc::Rc::try_unwrap(v) {
                Ok(v) => Ok(v),
                Err(v) => v.as_ref().try_clone(),
            },
        }
    }
}
impl core::ops::Deref for SharedRaster {
    type Target = Raster;
    fn deref(&self) -> &Raster {
        self.as_ref()
    }
}

pub(crate) enum ScratchVec<T> {
    Paid(BudgetVec<T>),
    Compatibility(Vec<T>),
}
impl<T> ScratchVec<T> {
    pub(crate) fn new(resources: Option<&ResourcesRef>) -> Self {
        match resources {
            Some(r) => Self::Paid(BudgetVec::new(r.clone(), ResourceKind::Scratch)),
            None => Self::Compatibility(Vec::new()),
        }
    }
    pub(crate) fn try_push(&mut self, value: T) -> Result<(), Error> {
        match self {
            Self::Paid(v) => v.try_push(value),
            Self::Compatibility(v) => {
                v.push(value);
                Ok(())
            }
        }
    }
    pub(crate) fn pop(&mut self) -> Option<T> {
        match self {
            Self::Paid(v) => v.pop(),
            Self::Compatibility(v) => v.pop(),
        }
    }
}
impl<T: Copy> ScratchVec<T> {
    pub(crate) fn try_filled(
        resources: Option<&ResourcesRef>,
        length: usize,
        value: T,
    ) -> Result<Self, Error> {
        let mut output = Self::new(resources);
        match &mut output {
            Self::Paid(v) => {
                v.try_reserve_exact(length)?;
                for index in 0..length {
                    if index % 256 == 0 {
                        resources
                            .unwrap()
                            .checkpoint((length - index).min(256) as u64)?;
                    }
                    v.try_push(value)?;
                }
            }
            Self::Compatibility(v) => {
                v.resize(length, value);
            }
        }
        Ok(output)
    }
    pub(crate) fn try_from_slice(
        resources: Option<&ResourcesRef>,
        values: &[T],
    ) -> Result<Self, Error> {
        let mut output = Self::new(resources);
        match &mut output {
            Self::Paid(v) => {
                v.try_reserve_exact(values.len())?;
                for (index, value) in values.iter().enumerate() {
                    if index % 256 == 0 {
                        resources
                            .unwrap()
                            .checkpoint((values.len() - index).min(256) as u64)?;
                    }
                    v.try_push(*value)?;
                }
            }
            Self::Compatibility(v) => v.extend_from_slice(values),
        }
        Ok(output)
    }
}
impl<T> core::ops::Deref for ScratchVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Paid(v) => v.as_slice(),
            Self::Compatibility(v) => v.as_slice(),
        }
    }
}
impl<T> core::ops::DerefMut for ScratchVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Paid(v) => v.as_mut_slice(),
            Self::Compatibility(v) => v.as_mut_slice(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Raster {
    storage: Storage,
    size: tiny_skia::IntSize,
    resources: Option<ResourcesRef>,
}

impl Raster {
    pub(crate) fn try_create(
        width: u32,
        height: u32,
        resources: Option<&ResourcesRef>,
    ) -> Result<Self, Error> {
        let invalid = || {
            let error = Error::new(FailureKind::RasterAllocation, LimitKind::Capacity);
            resources.map_or(error, |r| r.record(error))
        };
        let size = tiny_skia::IntSize::from_wh(width, height).ok_or_else(invalid)?;
        let length = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or_else(invalid)?;
        let storage = if let Some(resources) = resources {
            let mut bytes = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
            bytes.try_resize_zeroed(length)?;
            Storage::Paid(bytes)
        } else {
            Storage::Compatibility(tiny_skia::Pixmap::new(width, height).ok_or_else(invalid)?)
        };
        Ok(Self {
            storage,
            size,
            resources: resources.cloned(),
        })
    }

    pub(crate) fn resources(&self) -> Option<&ResourcesRef> {
        self.resources.as_ref()
    }
    pub(crate) fn width(&self) -> u32 {
        self.size.width()
    }
    pub(crate) fn height(&self) -> u32 {
        self.size.height()
    }
    pub(crate) fn data(&self) -> &[u8] {
        match &self.storage {
            Storage::Paid(bytes) => bytes.as_slice(),
            Storage::Compatibility(pixmap) => pixmap.data(),
        }
    }
    pub(crate) fn data_mut(&mut self) -> &mut [u8] {
        match &mut self.storage {
            Storage::Paid(bytes) => bytes.as_mut_slice(),
            Storage::Compatibility(pixmap) => pixmap.data_mut(),
        }
    }
    pub(crate) fn as_ref(&self) -> tiny_skia::PixmapRef<'_> {
        tiny_skia::PixmapRef::from_bytes(self.data(), self.width(), self.height())
            .expect("checked owned raster dimensions")
    }
    pub(crate) fn as_mut(&mut self) -> tiny_skia::PixmapMut<'_> {
        let (width, height) = (self.width(), self.height());
        tiny_skia::PixmapMut::from_bytes(self.data_mut(), width, height)
            .expect("checked owned raster dimensions")
    }
    pub(crate) fn checkpoint(&self, pixels: usize) -> Result<(), Error> {
        if let Some(resources) = &self.resources {
            resources.checkpoint(pixels as u64)?;
        }
        Ok(())
    }
    pub(crate) fn try_clone(&self) -> Result<Self, Error> {
        let mut result = Self::try_create(self.width(), self.height(), self.resources())?;
        for (from, to) in self
            .data()
            .chunks(16_384)
            .zip(result.data_mut().chunks_mut(16_384))
        {
            self.checkpoint(from.len().div_ceil(4))?;
            to.copy_from_slice(from);
        }
        Ok(result)
    }
    pub(crate) fn try_copy_region(&self, rect: tiny_skia::IntRect) -> Result<Self, Error> {
        let bounds = tiny_skia::IntRect::from_xywh(0, 0, self.width(), self.height()).unwrap();
        let rect = rect.intersect(&bounds).ok_or_else(|| {
            let e = Error::new(FailureKind::InvalidFilterRegion, LimitKind::None);
            self.resources().map_or(e, |r| r.record(e))
        })?;
        let mut result = Self::try_create(rect.width(), rect.height(), self.resources())?;
        let row_bytes = rect.width() as usize * 4;
        for y in 0..rect.height() as usize {
            let start = ((rect.y() as usize + y) * self.width() as usize + rect.x() as usize) * 4;
            for (from, to) in self.data()[start..start + row_bytes]
                .chunks(16_384)
                .zip(result.data_mut()[y * row_bytes..(y + 1) * row_bytes].chunks_mut(16_384))
            {
                self.checkpoint(from.len().div_ceil(4))?;
                to.copy_from_slice(from);
            }
        }
        Ok(result)
    }
    pub(crate) fn fill(&mut self, color: tiny_skia::Color) {
        let color = color.premultiply().to_color_u8();
        let pixel = [color.red(), color.green(), color.blue(), color.alpha()];
        let resources = self.resources.clone();
        for chunk in self.data_mut().chunks_mut(16_384) {
            if let Some(r) = &resources {
                if r.checkpoint(chunk.len().div_ceil(4) as u64).is_err() {
                    return;
                }
            }
            for value in chunk.chunks_exact_mut(4) {
                value.copy_from_slice(&pixel);
            }
        }
    }
    pub(crate) fn pixels_mut(&mut self) -> &mut [tiny_skia::PremultipliedColorU8] {
        bytemuck::cast_slice_mut(self.data_mut())
    }
    pub(crate) fn fill_rect(
        &mut self,
        rect: tiny_skia::Rect,
        paint: &tiny_skia::Paint,
        transform: tiny_skia::Transform,
        mask: Option<&tiny_skia::Mask>,
    ) {
        let resources = self.resources.clone();
        match resources.as_ref() {
            Some(resources) => {
                if let Err(error) = self
                    .as_mut()
                    .try_fill_rect(rect, paint, transform, mask, resources)
                {
                    resources.record(error);
                }
            }
            None => self.as_mut().fill_rect(rect, paint, transform, mask),
        }
    }
    pub(crate) fn draw_pixmap(
        &mut self,
        x: i32,
        y: i32,
        pixmap: tiny_skia::PixmapRef<'_>,
        paint: &tiny_skia::PixmapPaint,
        transform: tiny_skia::Transform,
        mask: Option<&tiny_skia::Mask>,
    ) {
        let resources = self.resources.clone();
        match resources.as_ref() {
            Some(resources) => {
                if let Err(error) = self
                    .as_mut()
                    .try_draw_pixmap(x, y, pixmap, paint, transform, mask, resources)
                {
                    resources.record(error);
                }
            }
            None => self
                .as_mut()
                .draw_pixmap(x, y, pixmap, paint, transform, mask),
        }
    }
    pub(crate) fn apply_mask(&mut self, mask: tiny_skia::MaskRef<'_>) {
        let resources = self.resources.clone();
        match resources.as_ref() {
            Some(r) => {
                if let Err(error) = self.as_mut().try_apply_mask_ref(mask, r) {
                    r.record(error);
                }
                if let Err(error) = r.checkpoint(0) {
                    r.record(error);
                }
            }
            None => self.as_mut().apply_mask_ref(mask),
        }
    }
}

enum MaskStorage {
    Paid(BudgetVec<u8>),
    Compatibility(tiny_skia::Mask),
}
pub(crate) struct RasterMask {
    storage: MaskStorage,
    size: tiny_skia::IntSize,
    resources: Option<ResourcesRef>,
}
impl RasterMask {
    pub(crate) fn try_new(
        width: u32,
        height: u32,
        resources: Option<&ResourcesRef>,
    ) -> Result<Self, Error> {
        let invalid = || {
            let e = Error::new(FailureKind::RasterAllocation, LimitKind::Capacity);
            resources.map_or(e, |r| r.record(e))
        };
        let size = tiny_skia::IntSize::from_wh(width, height).ok_or_else(invalid)?;
        let length = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(invalid)?;
        let storage = match resources {
            Some(r) => {
                let mut v = BudgetVec::new(r.clone(), ResourceKind::Scratch);
                v.try_resize_zeroed(length)?;
                MaskStorage::Paid(v)
            }
            None => {
                MaskStorage::Compatibility(tiny_skia::Mask::new(width, height).ok_or_else(invalid)?)
            }
        };
        Ok(Self {
            storage,
            size,
            resources: resources.cloned(),
        })
    }
    pub(crate) fn as_ref(&self) -> tiny_skia::MaskRef<'_> {
        let data = match &self.storage {
            MaskStorage::Paid(v) => v.as_slice(),
            MaskStorage::Compatibility(v) => v.data(),
        };
        tiny_skia::MaskRef::from_bytes(data, self.size.width(), self.size.height()).unwrap()
    }
    pub(crate) fn as_mut(&mut self) -> tiny_skia::MaskMut<'_> {
        let data = match &mut self.storage {
            MaskStorage::Paid(v) => v.as_mut_slice(),
            MaskStorage::Compatibility(v) => v.data_mut(),
        };
        tiny_skia::MaskMut::from_bytes(data, self.size.width(), self.size.height()).unwrap()
    }
    pub(crate) fn try_from_pixmap(
        pixmap: &Raster,
        kind: tiny_skia::MaskType,
    ) -> Result<Self, Error> {
        let mut output = Self::try_new(pixmap.width(), pixmap.height(), pixmap.resources())?;
        let bytes = match &mut output.storage {
            MaskStorage::Paid(v) => v.as_mut_slice(),
            MaskStorage::Compatibility(v) => v.data_mut(),
        };
        for (pixels, mask) in pixmap
            .as_ref()
            .pixels()
            .chunks(4096)
            .zip(bytes.chunks_mut(4096))
        {
            pixmap.checkpoint(pixels.len())?;
            for (p, value) in pixels.iter().zip(mask) {
                *value = match kind {
                    tiny_skia::MaskType::Alpha => p.alpha(),
                    tiny_skia::MaskType::Luminance => {
                        let mut r = f32::from(p.red()) / 255.0;
                        let mut g = f32::from(p.green()) / 255.0;
                        let mut b = f32::from(p.blue()) / 255.0;
                        let a = f32::from(p.alpha()) / 255.0;
                        if p.alpha() != 0 {
                            r /= a;
                            g /= a;
                            b /= a;
                        }
                        (((r * 0.2126 + g * 0.7152 + b * 0.0722) * a) * 255.0)
                            .clamp(0.0, 255.0)
                            .ceil() as u8
                    }
                };
            }
        }
        Ok(output)
    }
    pub(crate) fn invert(&mut self) -> Result<(), Error> {
        let resources = self.resources.clone();
        let data = match &mut self.storage {
            MaskStorage::Paid(v) => v.as_mut_slice(),
            MaskStorage::Compatibility(v) => v.data_mut(),
        };
        for chunk in data.chunks_mut(4096) {
            if let Some(r) = &resources {
                r.checkpoint(chunk.len() as u64)?;
            }
            for v in chunk {
                *v = 255 - *v;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "raster/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "raster/geometry_tests.rs"]
mod geometry_tests;
