// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Synchronous COM callback boundaries. Callback objects use exact paid storage;
//! they deactivate their borrowed target before Draw returns to the caller.
//! An unexpected retained COM reference keeps its allocation and grant alive,
//! and subsequent callbacks observe an inactive target rather than a stale borrow.

use markdown_memory::{Family, MemoryScope, OwnedBlock};
use markdown_mermaid::canonical::{
    CanonicalError, FontFamily, PathCommand, PathSink, PathVerb, Rect,
};
use merman_core::OperationControl;
use std::{
    ffi::c_void,
    mem::ManuallyDrop,
    ptr,
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
};
use windows::{
    Win32::Graphics::{
        Direct2D::Common::{
            D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN, D2D1_FIGURE_END, D2D1_FIGURE_END_CLOSED,
            D2D1_FILL_MODE, D2D1_PATH_SEGMENT, ID2D1SimplifiedGeometrySink,
            ID2D1SimplifiedGeometrySink_Vtbl,
        },
        DirectWrite::*,
    },
    core::{BOOL, GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface},
};
use windows_numerics::Vector2;

const OK: HRESULT = HRESULT(0);
const FAIL: HRESULT = HRESULT(0x80004005u32 as i32);
const POINTER: HRESULT = HRESULT(0x80004003u32 as i32);
const NO_INTERFACE: HRESULT = HRESULT(0x80004002u32 as i32);

fn com_callback<T: Copy>(failure: T, action: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(action)) {
        Ok(value) => value,
        Err(payload) => {
            // A caught payload may itself panic during Drop. Contain that
            // second unwind before returning through the non-unwinding COM ABI.
            if let Err(second) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(payload)))
            {
                std::mem::forget(second);
            }
            failure
        }
    }
}

// COM refcount arithmetic must never unwind through a Windows callback. A
// saturated count deliberately retains its paid allocation instead of wrapping
// to zero and freeing an object that still has outstanding references.
fn add_reference(references: &AtomicU32) -> u32 {
    match references.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1))
    }) {
        Ok(previous) => previous.saturating_add(1),
        Err(value) => value,
    }
}

fn release_reference(references: &AtomicU32) -> u32 {
    match references.try_update(Ordering::Release, Ordering::Relaxed, |value| {
        (value != u32::MAX).then(|| value.saturating_sub(1))
    }) {
        Ok(previous) => previous.saturating_sub(1),
        Err(value) => value,
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
}

struct GeometryState {
    target: Option<*mut (dyn PathSink + 'static)>,
    offset: (f64, f64),
    last: (f64, f64),
    first: (f64, f64),
    bounds: Rect,
    seen: bool,
    error: Option<CanonicalError>,
}

#[repr(C)]
struct Geometry {
    vtable: *const ID2D1SimplifiedGeometrySink_Vtbl,
    references: AtomicU32,
    thread: u32,
    control: OperationControl,
    state: Mutex<GeometryState>,
    allocation: ManuallyDrop<OwnedBlock>,
}

impl Geometry {
    fn create(
        scope: &MemoryScope,
        control: &OperationControl,
        target: Option<&mut dyn PathSink>,
    ) -> Result<ID2D1SimplifiedGeometrySink, CanonicalError> {
        let mut allocation = OwnedBlock::try_array::<Self>(scope, 1, Family::Metadata)?;
        let pointer = allocation.as_mut_ptr().cast::<Self>();
        // SAFETY: the target is borrowed only for this synchronous Draw scope.
        // finish() clears it under the same mutex on every completion path.
        let target = target.map(|sink| unsafe {
            std::mem::transmute::<*mut dyn PathSink, *mut (dyn PathSink + 'static)>(
                sink as *mut dyn PathSink,
            )
        });
        let object = Self {
            vtable: &GEOMETRY_VTABLE,
            references: AtomicU32::new(1),
            thread: unsafe { GetCurrentThreadId() },
            control: control.clone(),
            state: Mutex::new(GeometryState {
                target,
                offset: (0.0, 0.0),
                last: (0.0, 0.0),
                first: (0.0, 0.0),
                bounds: Rect::default(),
                seen: false,
                error: None,
            }),
            allocation: ManuallyDrop::new(allocation),
        };
        // SAFETY: paid/aligned storage is exactly Self, initialized before COM sees it.
        unsafe {
            pointer.write(object);
            Ok(ID2D1SimplifiedGeometrySink::from_raw(pointer.cast()))
        }
    }

    unsafe fn from_this<'a>(this: *mut c_void) -> &'a Self {
        unsafe { &*this.cast::<Self>() }
    }

    fn checkpoint(&self) -> Result<(), CanonicalError> {
        self.control
            .memory_scope()
            .ok_or(CanonicalError::Memory(
                markdown_memory::MemoryError::Unfunded,
            ))?
            .charge_work(1)?;
        self.control
            .checkpoint()
            .map_err(|_| CanonicalError::Cancelled)
    }

    fn finish(&self) -> Result<Rect, CanonicalError> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(error) => {
                let mut state = error.into_inner();
                state.target = None;
                state.error = Some(CanonicalError::InvalidGeometry);
                return Err(CanonicalError::InvalidGeometry);
            }
        };
        state.target = None;
        match state.error {
            Some(error) => Err(error),
            None => Ok(state.bounds),
        }
    }

    fn emit(&self, command: PathCommand) {
        if unsafe { GetCurrentThreadId() } != self.thread {
            if let Ok(mut state) = self.state.lock() {
                state.error = Some(CanonicalError::InvalidGeometry);
            }
            return;
        }
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.error.is_some() {
            return;
        }
        if let Err(error) = self.checkpoint() {
            state.error = Some(error);
            return;
        }
        let offset = state.offset;
        let point = |x: f64, y: f64| (x + offset.0, y + offset.1);
        let mut c = command;
        if c.verb != PathVerb::Close {
            (c.x1, c.y1) = point(c.x1, c.y1);
        }
        if matches!(c.verb, PathVerb::Quad | PathVerb::Cubic) {
            (c.x2, c.y2) = point(c.x2, c.y2);
        }
        if c.verb == PathVerb::Cubic {
            (c.x3, c.y3) = point(c.x3, c.y3);
        }
        if ![c.x1, c.y1, c.x2, c.y2, c.x3, c.y3]
            .into_iter()
            .all(f64::is_finite)
        {
            state.error = Some(CanonicalError::InvalidGeometry);
            return;
        }
        let prior = state.last;
        match c.verb {
            PathVerb::Move => {
                state.first = (c.x1, c.y1);
                state.last = state.first;
            }
            PathVerb::Line => {
                include(&mut state, prior);
                include(&mut state, (c.x1, c.y1));
                state.last = (c.x1, c.y1);
            }
            PathVerb::Cubic => {
                cubic_bounds(&mut state, prior, (c.x1, c.y1), (c.x2, c.y2), (c.x3, c.y3));
                state.last = (c.x3, c.y3);
            }
            PathVerb::Close => {
                let first = state.first;
                include(&mut state, prior);
                include(&mut state, first);
                state.last = first;
            }
            PathVerb::Quad => {
                state.error = Some(CanonicalError::InvalidGeometry);
                return;
            }
        }
        if let Some(target) = state.target {
            // SAFETY: same-thread check plus the mutex and finish()'s deactivation
            // bound this callback to the live exclusive PathSink borrow.
            let result = com_callback(Err(CanonicalError::InvalidGeometry), || unsafe {
                (&mut *target).push(c)
            });
            match result {
                Ok(()) => {}
                Err(error) => state.error = Some(error),
            }
        }
    }
}

fn include(state: &mut GeometryState, p: (f64, f64)) {
    if !state.seen {
        state.bounds = Rect {
            left: p.0,
            top: p.1,
            right: p.0,
            bottom: p.1,
        };
        state.seen = true;
    } else {
        state.bounds.left = state.bounds.left.min(p.0);
        state.bounds.right = state.bounds.right.max(p.0);
        state.bounds.top = state.bounds.top.min(p.1);
        state.bounds.bottom = state.bounds.bottom.max(p.1);
    }
}

fn cubic_bounds(
    state: &mut GeometryState,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
) {
    include(state, p0);
    include(state, p3);
    for (a, b, c, d) in [(p0.0, p1.0, p2.0, p3.0), (p0.1, p1.1, p2.1, p3.1)] {
        let aa = -a + 3.0 * b - 3.0 * c + d;
        let bb = 2.0 * (a - 2.0 * b + c);
        let cc = b - a;
        let mut roots = [f64::NAN; 2];
        if aa.abs() < 1e-12 {
            if bb.abs() > 1e-12 {
                roots[0] = -cc / bb;
            }
        } else {
            let discriminant = bb * bb - 4.0 * aa * cc;
            if discriminant >= 0.0 {
                let s = discriminant.sqrt();
                roots = [(-bb + s) / (2.0 * aa), (-bb - s) / (2.0 * aa)];
            }
        }
        for t in roots {
            if t > 0.0 && t < 1.0 {
                let s = 1.0 - t;
                let eval = |a: f64, b: f64, c: f64, d: f64| {
                    s * s * s * a + 3.0 * s * s * t * b + 3.0 * s * t * t * c + t * t * t * d
                };
                include(
                    state,
                    (eval(p0.0, p1.0, p2.0, p3.0), eval(p0.1, p1.1, p2.1, p3.1)),
                );
            }
        }
    }
}

unsafe extern "system" fn geometry_query(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    com_callback(FAIL, || {
        if iid.is_null() || out.is_null() {
            return POINTER;
        }
        unsafe {
            *out = ptr::null_mut();
            if *iid == ID2D1SimplifiedGeometrySink::IID || *iid == IUnknown::IID {
                *out = this;
                geometry_add(this);
                OK
            } else {
                NO_INTERFACE
            }
        }
    })
}
unsafe extern "system" fn geometry_add(this: *mut c_void) -> u32 {
    com_callback(u32::MAX, || {
        add_reference(&unsafe { Geometry::from_this(this) }.references)
    })
}
unsafe extern "system" fn geometry_release(this: *mut c_void) -> u32 {
    com_callback(0, || {
        let object = unsafe { Geometry::from_this(this) };
        let remaining = release_reference(&object.references);
        if remaining == 0 {
            std::sync::atomic::fence(Ordering::Acquire);
            unsafe {
                let object = this.cast::<Geometry>();
                let allocation = ManuallyDrop::take(&mut (*object).allocation);
                ptr::drop_in_place(object);
                drop(allocation);
            }
        }
        remaining
    })
}
unsafe extern "system" fn fill_mode(_: *mut c_void, _: D2D1_FILL_MODE) {}
unsafe extern "system" fn segment_flags(_: *mut c_void, _: D2D1_PATH_SEGMENT) {}
unsafe extern "system" fn begin(this: *mut c_void, p: Vector2, _: D2D1_FIGURE_BEGIN) {
    com_callback((), || {
        unsafe { Geometry::from_this(this) }.emit(PathCommand::move_to(p.X as f64, p.Y as f64));
    })
}
unsafe extern "system" fn lines(this: *mut c_void, points: *const Vector2, count: u32) {
    com_callback((), || {
        if count == 0 {
            return;
        }
        if points.is_null() || count > 65536 {
            let object = unsafe { Geometry::from_this(this) };
            if let Ok(mut state) = object.state.lock() {
                state.error = Some(CanonicalError::InvalidGeometry);
            }
            return;
        }
        for p in unsafe { std::slice::from_raw_parts(points, count as usize) } {
            unsafe { Geometry::from_this(this) }.emit(PathCommand::line_to(p.X as f64, p.Y as f64));
        }
    })
}
unsafe extern "system" fn beziers(
    this: *mut c_void,
    segments: *const D2D1_BEZIER_SEGMENT,
    count: u32,
) {
    com_callback((), || {
        if count == 0 {
            return;
        }
        if segments.is_null() || count > 65536 {
            let object = unsafe { Geometry::from_this(this) };
            if let Ok(mut state) = object.state.lock() {
                state.error = Some(CanonicalError::InvalidGeometry);
            }
            return;
        }
        for c in unsafe { std::slice::from_raw_parts(segments, count as usize) } {
            unsafe { Geometry::from_this(this) }.emit(PathCommand::cubic(
                c.point1.X as f64,
                c.point1.Y as f64,
                c.point2.X as f64,
                c.point2.Y as f64,
                c.point3.X as f64,
                c.point3.Y as f64,
            ));
        }
    })
}
unsafe extern "system" fn end(this: *mut c_void, end: D2D1_FIGURE_END) {
    com_callback((), || {
        if end == D2D1_FIGURE_END_CLOSED {
            unsafe { Geometry::from_this(this) }.emit(PathCommand::close());
        }
    })
}
unsafe extern "system" fn close(this: *mut c_void) -> HRESULT {
    com_callback(FAIL, || {
        let object = unsafe { Geometry::from_this(this) };
        match object.state.lock() {
            Ok(state) if state.error.is_none() => OK,
            _ => FAIL,
        }
    })
}
const GEOMETRY_VTABLE: ID2D1SimplifiedGeometrySink_Vtbl = ID2D1SimplifiedGeometrySink_Vtbl {
    base__: IUnknown_Vtbl {
        QueryInterface: geometry_query,
        AddRef: geometry_add,
        Release: geometry_release,
    },
    SetFillMode: fill_mode,
    SetSegmentFlags: segment_flags,
    BeginFigure: begin,
    AddLines: lines,
    AddBeziers: beziers,
    EndFigure: end,
    Close: close,
};

#[repr(C)]
struct Renderer {
    vtable: *const IDWriteTextRenderer_Vtbl,
    references: AtomicU32,
    geometry: ID2D1SimplifiedGeometrySink,
    collection: IDWriteFontCollection,
    baseline: f64,
    thread: u32,
    active: std::sync::atomic::AtomicBool,
    allocation: ManuallyDrop<OwnedBlock>,
}

impl Renderer {
    fn create(
        scope: &MemoryScope,
        collection: &IDWriteFontCollection,
        geometry: &ID2D1SimplifiedGeometrySink,
        baseline: f64,
    ) -> Result<IDWriteTextRenderer, CanonicalError> {
        let mut allocation = OwnedBlock::try_array::<Self>(scope, 1, Family::Metadata)?;
        let pointer = allocation.as_mut_ptr().cast::<Self>();
        let object = Self {
            vtable: &RENDERER_VTABLE,
            references: AtomicU32::new(1),
            geometry: geometry.clone(),
            collection: collection.clone(),
            baseline,
            thread: unsafe { GetCurrentThreadId() },
            active: std::sync::atomic::AtomicBool::new(true),
            allocation: ManuallyDrop::new(allocation),
        };
        unsafe {
            pointer.write(object);
            Ok(IDWriteTextRenderer::from_raw(pointer.cast()))
        }
    }
    unsafe fn from_this<'a>(this: *mut c_void) -> &'a Self {
        unsafe { &*this.cast::<Self>() }
    }
    fn fail(&self, error: CanonicalError) -> HRESULT {
        let geometry = unsafe { Geometry::from_this(self.geometry.as_raw()) };
        if let Ok(mut state) = geometry.state.lock()
            && state.error.is_none()
        {
            state.error = Some(error);
        }
        FAIL
    }
}

unsafe extern "system" fn renderer_query(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    com_callback(FAIL, || {
        if iid.is_null() || out.is_null() {
            return POINTER;
        }
        unsafe {
            *out = ptr::null_mut();
            if *iid == IDWriteTextRenderer::IID
                || *iid == IDWritePixelSnapping::IID
                || *iid == IUnknown::IID
            {
                *out = this;
                renderer_add(this);
                OK
            } else {
                NO_INTERFACE
            }
        }
    })
}
unsafe extern "system" fn renderer_add(this: *mut c_void) -> u32 {
    com_callback(u32::MAX, || {
        add_reference(&unsafe { Renderer::from_this(this) }.references)
    })
}
unsafe extern "system" fn renderer_release(this: *mut c_void) -> u32 {
    com_callback(0, || {
        let object = unsafe { Renderer::from_this(this) };
        let remaining = release_reference(&object.references);
        if remaining == 0 {
            std::sync::atomic::fence(Ordering::Acquire);
            unsafe {
                let object = this.cast::<Renderer>();
                let allocation = ManuallyDrop::take(&mut (*object).allocation);
                ptr::drop_in_place(object);
                drop(allocation);
            }
        }
        remaining
    })
}
unsafe extern "system" fn snapping(_: *mut c_void, _: *const c_void, value: *mut BOOL) -> HRESULT {
    com_callback(FAIL, || {
        if value.is_null() {
            POINTER
        } else {
            unsafe {
                *value = BOOL(1);
            }
            OK
        }
    })
}
unsafe extern "system" fn current_transform(
    _: *mut c_void,
    _: *const c_void,
    value: *mut DWRITE_MATRIX,
) -> HRESULT {
    com_callback(FAIL, || {
        if value.is_null() {
            POINTER
        } else {
            unsafe {
                *value = DWRITE_MATRIX {
                    m11: 1.0,
                    m12: 0.0,
                    m21: 0.0,
                    m22: 1.0,
                    dx: 0.0,
                    dy: 0.0,
                };
            }
            OK
        }
    })
}
unsafe extern "system" fn pixels(_: *mut c_void, _: *const c_void, value: *mut f32) -> HRESULT {
    com_callback(FAIL, || {
        if value.is_null() {
            POINTER
        } else {
            unsafe {
                *value = 1.0;
            }
            OK
        }
    })
}

unsafe extern "system" fn glyph_run(
    this: *mut c_void,
    _: *const c_void,
    x: f32,
    y: f32,
    _: DWRITE_MEASURING_MODE,
    run: *const DWRITE_GLYPH_RUN,
    _: *const DWRITE_GLYPH_RUN_DESCRIPTION,
    _: *mut c_void,
) -> HRESULT {
    com_callback(FAIL, || {
        let object = unsafe { Renderer::from_this(this) };
        let geometry = unsafe { Geometry::from_this(object.geometry.as_raw()) };
        if let Err(error) = geometry.checkpoint() {
            return object.fail(error);
        }
        if !object.active.load(Ordering::Acquire)
            || unsafe { GetCurrentThreadId() } != object.thread
            || run.is_null()
        {
            return object.fail(CanonicalError::InvalidGeometry);
        }
        let run = unsafe { &*run };
        if run.glyphCount == 0 {
            return OK;
        }
        if run.glyphCount > 65536 || run.glyphIndices.is_null() {
            return object.fail(CanonicalError::Limited);
        }
        let Some(face) = run.fontFace.as_ref() else {
            return object.fail(CanonicalError::FontUnavailable);
        };
        match allowed_face(&object.collection, face) {
            Ok(true) => {}
            Ok(false) => return object.fail(CanonicalError::FontUnavailable),
            Err(error) => return object.fail(error),
        }
        let indices =
            unsafe { std::slice::from_raw_parts(run.glyphIndices, run.glyphCount as usize) };
        if indices.contains(&0) {
            return object.fail(CanonicalError::FontUnavailable);
        }
        let face4 = match face.cast::<IDWriteFontFace4>() {
            Ok(face) => face,
            Err(error) => return object.fail(crate::host::directwrite_error(error)),
        };
        {
            for glyph in indices {
                if let Err(error) = geometry.checkpoint() {
                    return object.fail(error);
                }
                let formats = match unsafe {
                    face4.GetGlyphImageFormats(
                        *glyph,
                        run.fontEmSize.ceil() as u32,
                        run.fontEmSize.ceil() as u32,
                    )
                } {
                    Ok(formats) => formats,
                    Err(error) => return object.fail(crate::host::directwrite_error(error)),
                };
                if formats.0
                    & !(DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE.0 | DWRITE_GLYPH_IMAGE_FORMATS_CFF.0)
                    != 0
                {
                    return object.fail(CanonicalError::FontUnavailable);
                }
            }
        }
        {
            let Ok(mut state) = geometry.state.lock() else {
                return FAIL;
            };
            state.offset = (x as f64, y as f64 - object.baseline);
        }
        match unsafe {
            face.GetGlyphRunOutline(
                run.fontEmSize,
                run.glyphIndices,
                (!run.glyphAdvances.is_null()).then_some(run.glyphAdvances),
                (!run.glyphOffsets.is_null()).then_some(run.glyphOffsets),
                run.glyphCount,
                run.isSideways.as_bool(),
                run.bidiLevel % 2 == 1,
                &object.geometry,
            )
        } {
            Ok(()) => match geometry.state.lock() {
                Ok(state) if state.error.is_none() => OK,
                _ => FAIL,
            },
            Err(error) => object.fail(crate::host::directwrite_error(error)),
        }
    })
}

fn allowed_face(
    collection: &IDWriteFontCollection,
    face: &IDWriteFontFace,
) -> Result<bool, CanonicalError> {
    let result = (|| -> windows::core::Result<bool> {
        unsafe {
            let font = collection.GetFontFromFontFace(face)?;
            let family = font.GetFontFamily()?;
            let names = family.GetFamilyNames()?;
            if names.GetCount() > 64 {
                return Ok(false);
            }
            let mut buffer = [0u16; 128];
            for i in 0..names.GetCount() {
                let length = names.GetStringLength(i)? as usize;
                if length >= buffer.len() {
                    continue;
                }
                names.GetString(i, &mut buffer[..length + 1])?;
                for allowed in [
                    FontFamily::SegoeUi,
                    FontFamily::MicrosoftYaHeiUi,
                    FontFamily::SegoeUiSymbol,
                    FontFamily::SegoeUiEmoji,
                ] {
                    if buffer[..length]
                        .iter()
                        .copied()
                        .eq(allowed.name().encode_utf16())
                    {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
    })();
    result.map_err(crate::host::directwrite_error)
}

unsafe extern "system" fn underline(
    this: *mut c_void,
    _: *const c_void,
    x: f32,
    y: f32,
    line: *const DWRITE_UNDERLINE,
    _: *mut c_void,
) -> HRESULT {
    com_callback(FAIL, || {
        if line.is_null() {
            return POINTER;
        }
        let object = unsafe { Renderer::from_this(this) };
        let line = unsafe { &*line };
        decoration(object, x, y, line.width, line.offset, line.thickness)
    })
}
unsafe extern "system" fn strike(
    this: *mut c_void,
    _: *const c_void,
    x: f32,
    y: f32,
    line: *const DWRITE_STRIKETHROUGH,
    _: *mut c_void,
) -> HRESULT {
    com_callback(FAIL, || {
        if line.is_null() {
            return POINTER;
        }
        let object = unsafe { Renderer::from_this(this) };
        let line = unsafe { &*line };
        decoration(object, x, y, line.width, line.offset, line.thickness)
    })
}
fn decoration(
    object: &Renderer,
    x: f32,
    y: f32,
    width: f32,
    offset: f32,
    thickness: f32,
) -> HRESULT {
    if !object.active.load(Ordering::Acquire) || unsafe { GetCurrentThreadId() } != object.thread {
        return FAIL;
    }
    let geometry = unsafe { Geometry::from_this(object.geometry.as_raw()) };
    if let Ok(mut state) = geometry.state.lock() {
        state.offset = (x as f64, y as f64 - object.baseline);
    } else {
        return FAIL;
    }
    let top = offset as f64;
    let bottom = top + thickness as f64;
    for c in [
        PathCommand::move_to(0.0, top),
        PathCommand::line_to(width as f64, top),
        PathCommand::line_to(width as f64, bottom),
        PathCommand::line_to(0.0, bottom),
        PathCommand::close(),
    ] {
        geometry.emit(c);
    }
    match geometry.state.lock() {
        Ok(state) if state.error.is_none() => OK,
        _ => FAIL,
    }
}
unsafe extern "system" fn inline_object(
    this: *mut c_void,
    _: *const c_void,
    _: f32,
    _: f32,
    _: *mut c_void,
    _: BOOL,
    _: BOOL,
    _: *mut c_void,
) -> HRESULT {
    com_callback(FAIL, || {
        unsafe { Renderer::from_this(this) }.fail(CanonicalError::UnsupportedElement)
    })
}
const RENDERER_VTABLE: IDWriteTextRenderer_Vtbl = IDWriteTextRenderer_Vtbl {
    base__: IDWritePixelSnapping_Vtbl {
        base__: IUnknown_Vtbl {
            QueryInterface: renderer_query,
            AddRef: renderer_add,
            Release: renderer_release,
        },
        IsPixelSnappingDisabled: snapping,
        GetCurrentTransform: current_transform,
        GetPixelsPerDip: pixels,
    },
    DrawGlyphRun: glyph_run,
    DrawUnderline: underline,
    DrawStrikethrough: strike,
    DrawInlineObject: inline_object,
};

pub(crate) fn draw(
    scope: &MemoryScope,
    control: &OperationControl,
    collection: &IDWriteFontCollection,
    layout: &IDWriteTextLayout,
    baseline: f64,
    target: Option<&mut dyn PathSink>,
) -> Result<Rect, CanonicalError> {
    let geometry = Geometry::create(scope, control, target)?;
    let renderer = Renderer::create(scope, collection, &geometry, baseline)?;
    let result = unsafe { layout.Draw(None, &renderer, 0.0, 0.0) };
    // Deactivate before the borrow can end, including every HRESULT failure.
    unsafe { Renderer::from_this(renderer.as_raw()) }
        .active
        .store(false, Ordering::Release);
    let bounds = unsafe { Geometry::from_this(geometry.as_raw()) }.finish();
    bounds.and_then(|bounds| {
        result
            .map(|_| bounds)
            .map_err(crate::host::directwrite_error)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_memory::{AllocationAuthority, MemoryError};
    use std::{
        alloc::Layout,
        sync::{Arc, atomic::AtomicUsize},
    };

    #[derive(Default)]
    struct Authority(AtomicUsize);
    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, _: Family) -> Result<(), MemoryError> {
            self.0.fetch_add(layout.size(), Ordering::SeqCst);
            Ok(())
        }
        fn release(&self, layout: Layout, _: Family) {
            self.0.fetch_sub(layout.size(), Ordering::SeqCst);
        }
        fn failure(&self) -> Option<MemoryError> {
            None
        }
    }

    #[test]
    fn geometry_callback_contains_a_panicking_sink_and_its_payload_drop() {
        struct Payload(Arc<AtomicUsize>);
        impl Drop for Payload {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
                panic!("outline payload drop probe");
            }
        }
        struct PanickingSink {
            calls: usize,
            drops: Arc<AtomicUsize>,
        }
        impl PathSink for PanickingSink {
            fn push(&mut self, _: PathCommand) -> Result<(), CanonicalError> {
                self.calls += 1;
                std::panic::panic_any(Payload(self.drops.clone()));
            }
        }
        let authority = Arc::new(Authority::default());
        let scope = MemoryScope::new(authority.clone());
        let control = OperationControl::new().with_memory_scope(scope.clone());
        let drops = Arc::new(AtomicUsize::new(0));
        let mut sink = PanickingSink {
            calls: 0,
            drops: drops.clone(),
        };
        let geometry = Geometry::create(&scope, &control, Some(&mut sink)).unwrap();
        unsafe { begin(geometry.as_raw(), Vector2::default(), D2D1_FIGURE_BEGIN(0)) };
        assert_eq!(unsafe { close(geometry.as_raw()) }, FAIL);
        assert_eq!(
            unsafe { Geometry::from_this(geometry.as_raw()) }
                .finish()
                .err(),
            Some(CanonicalError::InvalidGeometry)
        );
        drop(geometry);
        assert_eq!(sink.calls, 1);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(authority.0.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn geometry_callback_contains_a_panicking_work_authority() {
        struct PanickingWork;
        impl AllocationAuthority for PanickingWork {
            fn reserve(&self, _: Layout, _: Family) -> Result<(), MemoryError> {
                Ok(())
            }
            fn release(&self, _: Layout, _: Family) {}
            fn failure(&self) -> Option<MemoryError> {
                None
            }
            fn charge_work(&self, _: usize) -> Result<(), MemoryError> {
                panic!("outline work authority probe");
            }
        }
        let scope = MemoryScope::new(Arc::new(PanickingWork));
        let control = OperationControl::new().with_memory_scope(scope.clone());
        let geometry = Geometry::create(&scope, &control, None).unwrap();
        unsafe { begin(geometry.as_raw(), Vector2::default(), D2D1_FIGURE_BEGIN(0)) };
        assert_eq!(unsafe { close(geometry.as_raw()) }, FAIL);
        assert_eq!(
            unsafe { Geometry::from_this(geometry.as_raw()) }
                .finish()
                .err(),
            Some(CanonicalError::InvalidGeometry)
        );
    }

    #[test]
    fn underline_and_strike_callbacks_abort_on_outline_budget_failure() {
        struct Limited(usize);
        impl PathSink for Limited {
            fn push(&mut self, _: PathCommand) -> Result<(), CanonicalError> {
                self.0 += 1;
                Err(CanonicalError::Limited)
            }
        }
        let factory: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }.unwrap();
        let mut collection = None;
        unsafe { factory.GetSystemFontCollection(&mut collection, false) }.unwrap();
        let collection = collection.unwrap();
        for strike_through in [false, true] {
            let authority = Arc::new(Authority::default());
            let scope = MemoryScope::new(authority.clone());
            let control = OperationControl::new().with_memory_scope(scope.clone());
            let mut sink = Limited(0);
            let geometry = Geometry::create(&scope, &control, Some(&mut sink)).unwrap();
            let renderer = Renderer::create(&scope, &collection, &geometry, 0.0).unwrap();
            let result = if strike_through {
                let line = DWRITE_STRIKETHROUGH {
                    width: 10.0,
                    thickness: 1.0,
                    offset: 1.0,
                    ..Default::default()
                };
                unsafe {
                    strike(
                        renderer.as_raw(),
                        ptr::null(),
                        0.0,
                        0.0,
                        &line,
                        ptr::null_mut(),
                    )
                }
            } else {
                let line = DWRITE_UNDERLINE {
                    width: 10.0,
                    thickness: 1.0,
                    offset: 1.0,
                    ..Default::default()
                };
                unsafe {
                    underline(
                        renderer.as_raw(),
                        ptr::null(),
                        0.0,
                        0.0,
                        &line,
                        ptr::null_mut(),
                    )
                }
            };
            assert_eq!(result, FAIL);
            assert!(matches!(
                unsafe { Geometry::from_this(geometry.as_raw()) }.finish(),
                Err(CanonicalError::Limited)
            ));
            unsafe { Renderer::from_this(renderer.as_raw()) }
                .active
                .store(false, Ordering::Release);
            drop(renderer);
            drop(geometry);
            assert_eq!(sink.0, 1, "the first rejected segment ends emission");
            assert_eq!(authority.0.load(Ordering::SeqCst), 0);
        }
    }
}
