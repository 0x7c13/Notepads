// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use crate::color::PainterMode;
use crate::control::{test_support::*, KernelError};
use markdown_svg_budget::{BudgetVec, Limits, ResourceKind};

struct Sink<'r> {
    r: &'r ResourcesRef,
    callbacks: usize,
    cache: u8,
    temporary_cache: bool,
}
impl ColorPainter for Sink<'_> {
    fn resources(&self) -> &ResourcesRef {
        self.r
    }
    fn push_transform(&mut self, _: Transform) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn pop_transform(&mut self) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn push_clip_glyph(&mut self, _: GlyphId) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn push_clip_box(&mut self, _: BoundingBox<f32>) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn pop_clip(&mut self) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn fill(&mut self, _: Brush<'_>) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn fill_glyph(&mut self, _: GlyphId, _: Option<Transform>, _: Brush<'_>) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn push_layer(&mut self, _: CompositeMode) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn pop_layer_with_mode(&mut self, _: CompositeMode) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn paint_cached_color_glyph(
        &mut self,
        glyph: GlyphId,
    ) -> Result<Result<PaintCachedColorGlyph, PaintError>, Error> {
        self.callbacks += 1;
        let _last_callback_owner = if self.temporary_cache {
            let mut value = BudgetVec::new(self.r.clone(), ResourceKind::Scratch);
            value.try_push(1u8)?;
            Some(value)
        } else {
            None
        };
        Ok(match self.cache {
            0 => Ok(PaintCachedColorGlyph::Ok),
            1 => Ok(PaintCachedColorGlyph::Unimplemented),
            _ => Err(PaintError::GlyphNotFound(glyph)),
        })
    }
    fn abort_color_glyph(&mut self) {}
}
fn kernel(r: &ResourcesRef, painter: &mut Sink<'_>) -> Result<Result<(), PaintError>, Error> {
    let bytes = include_bytes!("../tests/fixtures/color-control-v1/colr-normal-v1.ttf");
    let font = FontRef::try_new(bytes, &mut RequestControl(r))?.unwrap();
    let collection = ColorGlyphs::try_new(&font, r)?;
    let glyph = collection.try_get(GlyphId::new(19))?.unwrap();
    // Exercise the real inner kernel tail independently of the facade's tail.
    glyph.inner.try_paint(
        LocationRef::default(),
        &mut RequestPainter(painter),
        &mut RequestControl(r),
    )
}
#[test]
fn stop_owner_natural_release_is_observed_by_the_kernel_tail_itself() {
    let (r, e) = resources(None, Limits::default());
    let mut sink = Sink {
        r: &r,
        callbacks: 0,
        cache: 0,
        temporary_cache: false,
    };
    kernel(&r, &mut sink).unwrap().unwrap();
    let ordinal = e.lock().unwrap().release;
    assert!(ordinal > 1);
    assert!(sink.callbacks > 0);
    r.validate_retired().unwrap();
    let (r, e) = resources(Some(Fault::Release(ordinal)), Limits::default());
    let mut sink = Sink {
        r: &r,
        callbacks: 0,
        cache: 0,
        temporary_cache: false,
    };
    assert_eq!(kernel(&r, &mut sink).err(), Some(FAULT));
    assert!(sink.callbacks > 0);
    assert_eq!(e.lock().unwrap().injected, 1);
    assert_eq!(r.validate_retired(), Err(FAULT));
}
#[test]
fn cache_adapter_itself_checks_the_callback_last_drop_before_each_return_state() {
    for cache in [0, 1, 2] {
        let (r, e) = resources(Some(Fault::Release(1)), Limits::default());
        let mut sink = Sink {
            r: &r,
            callbacks: 0,
            cache,
            temporary_cache: true,
        };
        let value = RequestPainter(&mut sink)
            .paint_cached_color_glyph(GlyphId::new(3), &mut RequestControl(&r));
        assert!(matches!(value,Err(KernelError::Control(error))if error==FAULT));
        assert_eq!(sink.callbacks, 1);
        assert_eq!(e.lock().unwrap().injected, 1);
        assert_eq!(r.validate_retired(), Err(FAULT));
    }
}
